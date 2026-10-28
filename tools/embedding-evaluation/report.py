"""Render measured outcomes without tuning the frozen experiment or selecting a winner."""
from __future__ import annotations

import json
import random
import shutil
import statistics
from pathlib import Path

from common import MODELS, ROOT, WORK, check_frozen, fixture

frozen = check_frozen()
raw, docs = fixture()
queries = {q["id"]: q for q in raw["queries"]}
results = {}
archive = Path(__file__).with_name("results")
archive.mkdir(exist_ok=True)
for name in MODELS:
    source = WORK / (name + ".json")
    if source.exists():
        result = json.loads(source.read_text())
        assert result["frozen"] == frozen
        results[name] = result
        shutil.copyfile(source, archive / source.name)
    elif (WORK / (name + "-access.json")).exists():
        shutil.copyfile(WORK / (name + "-access.json"), archive / (name + "-access.json"))
for name in ("frozen.json", "environment.lock.txt"):
    shutil.copyfile(WORK / name, archive / name)


def mean(rows, method, metric):
    values = [q["metrics"][method][metric] for q in rows if q["metrics"][method] is not None]
    return statistics.mean(values) if values else None


def percentage(value):
    return "—" if value is None else f"{value * 100:.1f}%"


lines = ["# Embedding-model evaluation — 25 October 2026", "",
         "**Status: " + ("both candidates measured" if len(results) == 2 else "two-model comparison in progress") + ". No production model is selected.**", "",
         "The owner designated the available Mac as the reference device. This local screening run uses fictional correspondence, not the owner's mail. The app is unchanged by this experiment.", "",
         "## Protocol and evidence", "",
         f"Frozen before inference at {frozen['frozen_at_utc']}. Fixture SHA-256: `{frozen['inputs']['tools/embedding-evaluation/fixture.json']}`.", "",
         f"The corpus contains {len(docs)} occurrences: 50 eligible passages and four deliberately removed, expired, unavailable or other-account fixtures. There are 48 queries: eight development and 40 test queries. Thirty-five test queries have relevant passages; five deliberately have no answer (including a filter with no eligible source). Relevance grades and splits were authored before inference and were not adjusted after inspecting rankings.", "",
         "Both candidates use the same sources and judgments, 512 dimensions, truncation followed by L2 normalization, float32 and the publisher's retrieval preparation. Qwen uses a fixed correspondence retrieval instruction; Gemma uses its documented query/title/text prompts. Each model must run alone through pinned Sentence Transformers/PyTorch on Metal, with no remote code or inference. The [protocol](../tools/embedding-evaluation/README.md), [fixture](../tools/embedding-evaluation/fixture.json), runtime lock and raw per-query evidence under `tools/embedding-evaluation/results/` make the run reviewable.", "",
         "The owner amended the second candidate after the initial Qwen pilot: use Apache-2.0 EmbeddingGemma 2 instead of gated EmbeddingGemma-300M. Google supports a text-only configuration with vision/audio encoders disabled (about 270M active parameters versus 740M full multimodal). Both models are rerun with the same updated runtime required by Gemma 2. Corpus, queries, judgments, retrieval instructions, fusion and acceptance thresholds are unchanged. The initial Qwen result was already observed; this is disclosed rather than describing it as unseen. Original pilot metadata is retained separately under `results/pilot-v1/`.", "",
         "Lexical proxy means the current client's all-terms/phrase candidate behavior reproduced on these fixture strings, ordered by synthetic source ID. It does not execute the encrypted client, reproduce UI ordering or measure current-app latency. BM25 is an additional diagnostic; it is not implemented in the app. Hybrid is fixed equal-weight reciprocal-rank fusion of BM25 and semantic retrieval (constant 60, top 50), with quoted phrases retained as constraints. Fixture eligibility and explicit filters precede ranking for every method. These are evaluation contracts, not newly implemented product filters.", ""]

for name, (model, revision) in MODELS.items():
    lines.extend([f"## {model}", "", f"Pinned checkpoint: `{revision}`.", ""])
    if name not in results:
        lines.extend(["**Not measured in the amended run yet.** No estimate is presented as measurement.", ""])
        continue
    result = results[name]
    rows = [q for q in result["queries"] if q["split"] == "test" and q["relevant"]]
    lines.extend(["### Reference device and runtime", ""])
    lines.extend("- " + row for row in result["system_before"]["hardware_os"])
    lines.extend(["", f"Python {result['python']}; " + ", ".join(f"{package} {version}" for package, version in result["packages"].items()) + ".",
                  f"Loaded parameters: {result['profile']['parameter_count']:,}; disabled encoders: {', '.join(result['profile']['disabled_encoders']) or 'none'}.",
                  "On AC power with other applications open. Power and thermal snapshots before/after the run are retained in the raw result. This is a desktop screening result, not a controlled battery or native-app responsiveness measurement.", "",
                  "### Retrieval on 35 positive test queries", "",
                  "| Method | Relevant first | Recall@5 | MRR@10 | nDCG@10 |",
                  "|---|---:|---:|---:|---:|"])
    labels = {"lexical_proxy": "Current lexical behavior proxy", "bm25_diagnostic": "BM25 diagnostic", "semantic": "Embedding alone", "hybrid": "BM25 + embedding"}
    for method, label in labels.items():
        lines.append(f"| {label} | {percentage(mean(rows, method, 'hit1'))} | {percentage(mean(rows, method, 'recall5'))} | {mean(rows, method, 'mrr10'):.3f} | {mean(rows, method, 'ndcg10'):.3f} |")
    lines.extend(["", "Recall@5 counts all judged relevant passages, including useful related evidence. MRR measures the reciprocal position of the first relevant item; nDCG uses the graded relevance judgments. Small, agent-authored samples are not evidence of general correspondence quality.", "",
                  "| Test category | Queries | Embedding Recall@5 | Embedding relevant first | Hybrid relevant first |",
                  "|---|---:|---:|---:|---:|"])
    for category in sorted({q["category"] for q in rows}):
        group = [q for q in rows if q["category"] == category]
        lines.append(f"| {category} | {len(group)} | {percentage(mean(group, 'semantic', 'recall5'))} | {percentage(mean(group, 'semantic', 'hit1'))} | {percentage(mean(group, 'hybrid', 'hit1'))} |")
    lines.extend(["", "### Material retrieval failures", "",
                  "Every test query for which the embedding's first result was not judged relevant is listed below. These judgments are reviewable in the fixture; no failure was removed from the aggregate.", "",
                  "| Query | Intended target | First embedding result | Target position |",
                  "|---|---|---|---:|"])
    titles = {doc["id"]: doc["title"] for doc in docs}
    for q in rows:
        if q["metrics"]["semantic"]["hit1"]:
            continue
        ranking = q["rankings"]["semantic"]
        target = max(q["relevant"], key=q["relevant"].get)
        position = str(ranking.index(target) + 1) if target in ranking else ">10"
        lines.append(f"| {q['query']}: {queries[q['query']]['text'].replace('|', '/')} | {target}: {titles[target]} | {ranking[0]}: {titles[ranking[0]]} | {position} |")
    lines.extend(["", "The fixture challenges refund versus replacement, a rejected versus agreed delay, precise identifiers, cross-language time/location changes, an implication versus its converse, and a final decision buried in a longer message. The table identifies this model's actual first-result failures. Similarity is evidence of related subject matter, not a factual interpretation or proof that a passage answers the query.", "",
                  "### No-answer queries", "",
                  "| Query | Highest eligible cosine | Top occurrence |",
                  "|---|---:|---|"])
    for q in result["queries"]:
        if q["split"] != "test" or q["relevant"]:
            continue
        scores = q["semantic_top_scores"]
        lines.append(f"| {q['query']}: {queries[q['query']]['text']} | {scores[0]['cosine']:.3f} | {scores[0]['id']}: {titles[scores[0]['id']]} |" if scores else f"| {q['query']}: {queries[q['query']]['text']} | — | No eligible occurrence |")
    lines.extend(["", "The model ranks unrelated passages even when the corpus has no answer. No confidence threshold was calibrated using the test set; production absence/uncertainty behavior remains open.", "",
                  "### Measured resource cost", ""])
    p = result["performance"]
    conservative = max(0, p["sampled_peak_rss_bytes"] - p["base_process_rss_bytes"]) + p["sampled_peak_metal_driver_bytes"]
    gib = lambda value: value / 1024**3
    lines.extend([f"- Cached checkpoint load: {p['cached_load_seconds']:.2f} seconds (separate from package import and download).",
                  f"- Initial corpus: {p['document_count']} passages in {p['index_seconds']:.2f} seconds, {p['documents_per_second']:.1f} passages/second; input tokens min/median/max {p['tokens']['min']}/{p['tokens']['median']:.0f}/{p['tokens']['max']}.",
                  f"- Sustained batch-four workload: 128 repetitions of two long fictional passages, {p['throughput_workload_token_range'][0]}–{p['throughput_workload_token_range'][1]} tokens, {p['throughput_workload_seconds']:.2f} seconds, {p['throughput_workload_passages_per_second']:.1f} passages/second. Repetition measures throughput, not retrieval quality or large-corpus scaling.",
                  f"- Warm single-query encoding: median {p['query_p50_seconds'] * 1000:.1f} ms, p95 {p['query_p95_seconds'] * 1000:.1f} ms over 48 sequential timings (first 16 fixture queries, three repetitions, MPS synchronized). These times exclude vector-store search, decryption and UI work.",
                  f"- Sampled process RSS peak {gib(p['sampled_peak_rss_bytes']):.2f} GiB; OS process maximum {gib(p['os_process_maxrss_bytes']):.2f} GiB. Sampled Metal-driver peak {gib(p['sampled_peak_metal_driver_bytes']):.2f} GiB, active Metal allocation {gib(p['sampled_peak_metal_allocated_bytes']):.2f} GiB.",
                  f"- Predeclared conservative incremental RSS + Metal-driver sum: {gib(conservative):.2f} GiB. Unified-memory accounting can double-count; this is not a measurement of unique physical memory or an app memory budget.",
                  "- Battery/energy and foreground app responsiveness: not measured. No quantized, Core ML, MLX, GGUF or mobile runtime was evaluated.", "",
                  "### Frozen acceptance gates", "",
                  "| Gate | Outcome |",
                  "|---|---|"])
    exact_rows = [q for q in rows if q["category"] in ("identifiers", "quoted_phrase")]
    gates = [
        ("No fixture eligibility/filter leakage", len(result["eligibility_violations"]) == 0, f"{len(result['eligibility_violations'])} violations"),
        ("Embedding Recall@5 ≥90% and MRR@10 ≥0.75", mean(rows, "semantic", "recall5") >= .90 and mean(rows, "semantic", "mrr10") >= .75,
         f"{percentage(mean(rows, 'semantic', 'recall5'))}; {mean(rows, 'semantic', 'mrr10'):.3f}"),
        ("Hybrid exact identifier/phrase hit@1 ≥ lexical proxy", mean(exact_rows, "hybrid", "hit1") >= mean(exact_rows, "lexical_proxy", "hit1"),
         f"hybrid {percentage(mean(exact_rows, 'hybrid', 'hit1'))}, lexical {percentage(mean(exact_rows, 'lexical_proxy', 'hit1'))}"),
        ("Warm query p95 ≤1 second", p["query_p95_seconds"] <= 1, f"{p['query_p95_seconds']:.3f} s"),
        ("Cached load ≤15 seconds", p["cached_load_seconds"] <= 15, f"{p['cached_load_seconds']:.2f} s"),
        ("Long-passage throughput ≥2 passages/second", p["throughput_workload_passages_per_second"] >= 2, f"{p['throughput_workload_passages_per_second']:.1f}/s"),
        ("Conservative memory sum <6 GiB", conservative < 6 * 1024**3, f"{gib(conservative):.2f} GiB (can double-count unified memory)"),
    ]
    for label, passed, detail in gates:
        lines.append(f"| {label} | {'PASS' if passed else 'FAIL'} — {detail} |")
    lines.extend(["", "Passing the synthetic eligibility gate validates this fixture harness's prefilter, not the production client's ciphertext, concurrency or authorization boundaries. A failed resource or exact-retrieval gate is not waived because an aggregate semantic score passes.", ""])

if len(results) == 2:
    left = {q["query"]: q for q in results["qwen"]["queries"] if q["split"] == "test" and q["relevant"]}
    right = {q["query"]: q for q in results["gemma2"]["queries"] if q["split"] == "test" and q["relevant"]}
    assert left.keys() == right.keys()
    delta = [left[key]["metrics"]["semantic"]["mrr10"] - right[key]["metrics"]["semantic"]["mrr10"] for key in sorted(left)]
    rng = random.Random(20261025)
    samples = sorted(statistics.mean(rng.choices(delta, k=len(delta))) for _ in range(5000))
    lines.extend(["## Paired uncertainty", "", f"Qwen minus Gemma mean test-query MRR difference: {statistics.mean(delta):.3f}; paired 5,000-resample bootstrap interval [{samples[125]:.3f}, {samples[4874]:.3f}] (seed 20261025). This interval describes these agent-authored queries, not general users or domains.", ""])

lines.extend(["## Decision and next boundary", "",
              "Prefer EmbeddingGemma 2 text-only as the candidate for the next integration experiment on this Mac: its sampled resource cost is lower and sustained long-passage throughput higher, while retrieval is competitive in this small screen. The paired MRR interval includes zero, so do not claim a demonstrated general retrieval-quality win. First-result failures still occur, including opposite meanings; a 100% top-five result is not 100% first-result accuracy. This recommendation does not select or ship a production model.", "",
              "These runs screen the two permissively licensed candidates for local text retrieval. Material ranking failures, the exact-match regression gate, conservative memory accounting and the paired uncertainty interval must govern interpretation; no candidate becomes a production choice merely by passing aggregate recall. The original 300M model was not measured and its provider-specific terms are not being accepted for this comparison.", "",
              "Before a production choice, review relevance judgments with the owner, expand independent held-out correspondence and hard negatives, evaluate exact-identifier precedence and abstention on a new development/test split, and measure an actual bounded Apple runtime (including quantization parity, cancellation/lock behavior and foreground interaction). Preserve the float32 baseline for conversion checks. Model evaluation does not implement semantic or structured search in the member app.", "",
              "Official sources: [Qwen model and required query preparation](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B), [EmbeddingGemma 2 checkpoint, text-only loading, prompts and precision](https://huggingface.co/google/embeddinggemma-2), [Google's Apache-2.0 license statement](https://ai.google.dev/gemma/docs/embeddinggemma). These license declarations identify the evaluated artifacts; this report does not attempt a general interpretation of the older Gemma terms.", ""])
destination = ROOT / "docs/embedding-model-evaluation.md"
destination.write_text("\n".join(lines))
print(destination)
