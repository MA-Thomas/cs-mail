"""Offline model experiment on fictional passages, not a production search adapter."""
from __future__ import annotations

import argparse
import importlib.metadata
import json
import os
import platform
import resource
import statistics
import subprocess
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

from common import (MODELS, QWEN_PROMPT, WORK, bm25_rank, check_frozen, digest,
                    eligible, fixture, fused_rank, lexical_rank, metrics, phrase,
                    source_text, write_json)

# Enforced before importing Hugging Face. Inference must use the already downloaded
# local snapshot and never contact a model service or fetch remote implementation code.
os.environ.update(HF_HOME=str(WORK / "huggingface"), HF_HUB_OFFLINE="1",
                  TRANSFORMERS_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1",
                  DO_NOT_TRACK="1", TOKENIZERS_PARALLELISM="false")
os.environ.pop("PYTORCH_ENABLE_MPS_FALLBACK", None)

parser = argparse.ArgumentParser()
parser.add_argument("--model", choices=MODELS, required=True)
args = parser.parse_args()
frozen = check_frozen()
raw, docs = fixture()
queries = raw["queries"]
model_id, revision = MODELS[args.model]
snapshot = WORK / "huggingface/hub" / ("models--" + model_id.replace("/", "--")) / "snapshots" / revision
if not (snapshot / "model.safetensors").exists():
    raise SystemExit("Pinned checkpoint missing. Run download.py for this model; no inference was attempted.")
result_path = WORK / (args.model + ".json")
assert not result_path.exists(), "Do not overwrite an observed model result"

import numpy as np
import psutil
import torch
from sentence_transformers import SentenceTransformer

assert torch.backends.mps.is_available(), "Reference experiment requires MPS; do not silently switch devices"
torch.set_num_threads(4)
torch.set_num_interop_threads(1)
process = psutil.Process()
base_rss = process.memory_info().rss
sample = {"rss": base_rss, "metal_driver": 0, "metal_allocated": 0}
finished = threading.Event()


def monitor():
    while not finished.wait(0.05):
        sample["rss"] = max(sample["rss"], process.memory_info().rss)
        sample["metal_driver"] = max(sample["metal_driver"], torch.mps.driver_allocated_memory())
        sample["metal_allocated"] = max(sample["metal_allocated"], torch.mps.current_allocated_memory())


def system_state():
    hardware = subprocess.check_output(["system_profiler", "SPHardwareDataType", "SPSoftwareDataType"], text=True, stderr=subprocess.DEVNULL)
    allowed = ("Model Name:", "Model Identifier:", "Chip:", "Memory:", "Total Number of Cores:", "System Version:", "Kernel Version:")
    return {"hardware_os": [line.strip() for line in hardware.splitlines() if any(key in line for key in allowed)],
            "power": subprocess.check_output(["pmset", "-g", "batt"], text=True),
            "thermal": subprocess.check_output(["pmset", "-g", "therm"], text=True, stderr=subprocess.STDOUT)}


print("Reference experiment:", model_id, revision, flush=True)
state_before = system_state()
thread = threading.Thread(target=monitor, daemon=True)
thread.start()
start = time.perf_counter()
options = {"config_kwargs": {"vision_config": None, "audio_config": None}} if args.model == "gemma2" else {}
model = SentenceTransformer(str(snapshot), device="mps", trust_remote_code=False,
                            local_files_only=True, model_kwargs={"dtype": torch.float32}, **options)
torch.mps.synchronize()
load_seconds = time.perf_counter() - start
model.max_seq_length = 2048
assert all(param.dtype == torch.float32 for param in model.parameters())
if args.model == "gemma2":
    assert sum(param.numel() for param in model.parameters()) < 350_000_000, "Text-only encoder configuration was not applied"
    assert not any("vision_tower" in name or "audio_tower" in name for name, _ in model.named_parameters())


def prepared_document(doc):
    if args.model == "gemma2":
        return "title: " + doc["title"] + " | text: " + doc["text"]
    return source_text(doc)


def encode(texts, query=False):
    prompt = (QWEN_PROMPT if args.model == "qwen" else "task: search result | query: ") if query else ""
    # Count before inference: silent truncation would invalidate the comparison.
    lengths = [len(model.tokenizer.encode(prompt + text, add_special_tokens=True)) for text in texts]
    assert max(lengths) <= model.max_seq_length, "Input truncation is not allowed"
    with torch.inference_mode():
        vectors = model.encode(texts, prompt=prompt, batch_size=4, show_progress_bar=False,
                               convert_to_numpy=True, normalize_embeddings=False)
    torch.mps.synchronize()
    assert vectors.ndim == 2 and vectors.shape[1] >= 512 and np.isfinite(vectors).all()
    vectors = vectors[:, :512].astype(np.float32)
    norm = np.linalg.norm(vectors, axis=1, keepdims=True)
    assert np.isfinite(norm).all() and (norm > 0).all()
    return vectors / norm, lengths


# Warm the kernel before separating sustained indexing from initial use.
encode(["Local fictional correspondence warmup."], query=True)
torch.mps.synchronize()
start = time.perf_counter()
cpu_before = process.cpu_times()
documents, lengths = encode([prepared_document(doc) for doc in docs])
index_seconds = time.perf_counter() - start
cpu_after = process.cpu_times()
print("Document inference complete:", len(docs), "passages in", round(index_seconds, 3), "seconds", flush=True)
query_vectors, query_lengths = encode([query["text"] for query in queries], query=True)
similarity = query_vectors @ documents.T
assert similarity.shape == (len(queries), len(docs)) and np.isfinite(similarity).all()
rankings = []
violations = []
for number, query in enumerate(queries):
    permitted = [i for i, doc in enumerate(docs) if eligible(doc, query, raw["now"])]
    corpus = [docs[i] for i in permitted]
    semantic = sorted(permitted, key=lambda i: (-float(similarity[number, i]), docs[i]["id"]))
    semantic_ids = [docs[i]["id"] for i in semantic]
    lexical = lexical_rank(corpus, query)
    bm25 = bm25_rank(corpus, query)
    hybrid = fused_rank(bm25, semantic_ids)
    exact = phrase(query)
    if exact is not None:
        allowed = {doc["id"] for doc in corpus if exact in source_text(doc).lower()}
        hybrid = [identifier for identifier in hybrid if identifier in allowed]
    methods = {"lexical_proxy": lexical, "bm25_diagnostic": bm25, "semantic": semantic_ids, "hybrid": hybrid}
    permitted_ids = {doc["id"] for doc in corpus}
    for method, ranking in methods.items():
        violations.extend((query["id"], method, identifier) for identifier in ranking if identifier not in permitted_ids)
    rankings.append({"query": query["id"], "split": query["split"], "category": query["category"],
                     "relevant": query["relevant"], "eligible_count": len(corpus),
                     "rankings": {method: ranking[:10] for method, ranking in methods.items()},
                     "metrics": {method: metrics(ranking, query["relevant"]) for method, ranking in methods.items()},
                     "semantic_top_scores": [{"id": docs[i]["id"], "cosine": float(similarity[number, i])} for i in semantic[:5]]})

# Single query latency includes tokenization, Metal inference, normalization and CPU
# transfer. Repeat a fixed range in fixture order, never pick favorable queries.
latencies = []
for repeat in range(3):
    for query in queries[:16]:
        start = time.perf_counter()
        encode([query["text"]], query=True)
        latencies.append(time.perf_counter() - start)
    print("Latency pass", repeat + 1, "complete", flush=True)

# Fixed 128 passages in the memorandum's intended approximate length range. They
# are repeated synthetic long passages for throughput only, not additional evidence
# for retrieval quality or corpus scale. No reuse of the vectors for ranking.
long_docs = [doc for doc in docs if doc["id"] in ("d47", "d49")]
workload = [prepared_document(long_docs[i % 2]) for i in range(128)]
start = time.perf_counter()
_, workload_lengths = encode(workload)
sustained_seconds = time.perf_counter() - start
state_after = system_state()
finished.set()
thread.join()
artifacts = {str(path.relative_to(snapshot)): digest(path) for path in sorted(snapshot.rglob("*")) if path.is_file()}
versions = {package: importlib.metadata.version(package) for package in ("torch", "transformers", "sentence-transformers", "numpy", "psutil", "huggingface-hub", "tokenizers", "safetensors", "pillow", "torchvision")}
result = {"model": model_id, "revision": revision, "timestamp_utc": datetime.now(timezone.utc).isoformat(),
          "frozen": frozen, "system_before": state_before, "system_after": state_after,
          "python": platform.python_version(), "packages": versions, "artifacts_sha256": artifacts,
          "profile": {"device": "mps", "dtype": "float32", "dimensions": 512, "normalization": "truncate_then_l2",
                      "query_prompt": QWEN_PROMPT if args.model == "qwen" else "task: search result | query: ",
                      "document_preparation": "subject + two newlines + body" if args.model == "qwen" else "title: {subject} | text: {body}",
                      "batch": 4, "max_tokens": 2048, "threads": 4, "remote_code": False,
                      "modules": str(model), "parameter_count": sum(p.numel() for p in model.parameters()),
                      "disabled_encoders": ["vision", "audio"] if args.model == "gemma2" else [], "offline": True},
          "performance": {"cached_load_seconds": load_seconds, "document_count": len(docs), "index_seconds": index_seconds,
                          "documents_per_second": len(docs) / index_seconds, "tokens": {"min": min(lengths), "median": statistics.median(lengths), "max": max(lengths)},
                          "query_token_max": max(query_lengths), "query_latency_seconds": latencies,
                          "query_p50_seconds": float(np.percentile(latencies, 50)), "query_p95_seconds": float(np.percentile(latencies, 95)),
                          "index_cpu_seconds": (cpu_after.user + cpu_after.system) - (cpu_before.user + cpu_before.system),
                          "throughput_workload_passages": 128, "throughput_workload_token_range": [min(workload_lengths), max(workload_lengths)],
                          "throughput_workload_seconds": sustained_seconds, "throughput_workload_passages_per_second": 128 / sustained_seconds,
                          "base_process_rss_bytes": base_rss, "sampled_peak_rss_bytes": sample["rss"],
                          "sampled_peak_metal_driver_bytes": sample["metal_driver"], "sampled_peak_metal_allocated_bytes": sample["metal_allocated"],
                          "os_process_maxrss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                          "battery_energy": "not_measured_AC_power", "native_app_responsiveness": "not_measured"},
          "eligibility_violations": violations, "queries": rankings}
check_frozen()
np.savez(WORK / (args.model + "-fictional-vectors.npz"), documents=documents, queries=query_vectors)
write_json(result_path, result)
print("Saved:", result_path, flush=True)
