# Embedding-model evaluation — 25 October 2026

**Status: both candidates measured. No production model is selected.**

The owner designated the available Mac as the reference device. This local screening run uses fictional correspondence, not the owner's mail. The app is unchanged by this experiment.

## Protocol and evidence

Frozen before inference at 2026-10-25T19:31:22.030940+00:00. Fixture SHA-256: `e4c49b870fcdf8142069af3f149a821c19744f9dc4743ac2d89eec1c3efceb5c`.

The corpus contains 54 occurrences: 50 eligible passages and four deliberately removed, expired, unavailable or other-account fixtures. There are 48 queries: eight development and 40 test queries. Thirty-five test queries have relevant passages; five deliberately have no answer (including a filter with no eligible source). Relevance grades and splits were authored before inference and were not adjusted after inspecting rankings.

Both candidates use the same sources and judgments, 512 dimensions, truncation followed by L2 normalization, float32 and the publisher's retrieval preparation. Qwen uses a fixed correspondence retrieval instruction; Gemma uses its documented query/title/text prompts. Each model must run alone through pinned Sentence Transformers/PyTorch on Metal, with no remote code or inference. The [protocol](../tools/embedding-evaluation/README.md), [fixture](../tools/embedding-evaluation/fixture.json), runtime lock and raw per-query evidence under `tools/embedding-evaluation/results/` make the run reviewable.

The owner amended the second candidate after the initial Qwen pilot: use Apache-2.0 EmbeddingGemma 2 instead of gated EmbeddingGemma-300M. Google supports a text-only configuration with vision/audio encoders disabled (about 270M active parameters versus 740M full multimodal). Both models are rerun with the same updated runtime required by Gemma 2. Corpus, queries, judgments, retrieval instructions, fusion and acceptance thresholds are unchanged. The initial Qwen result was already observed; this is disclosed rather than describing it as unseen. Original pilot metadata is retained separately under `results/pilot-v1/`.

Lexical proxy means the current client's all-terms/phrase candidate behavior reproduced on these fixture strings, ordered by synthetic source ID. It does not execute the encrypted client, reproduce UI ordering or measure current-app latency. BM25 is an additional diagnostic; it is not implemented in the app. Hybrid is fixed equal-weight reciprocal-rank fusion of BM25 and semantic retrieval (constant 60, top 50), with quoted phrases retained as constraints. Fixture eligibility and explicit filters precede ranking for every method. These are evaluation contracts, not newly implemented product filters.

## Qwen/Qwen3-Embedding-0.6B

Pinned checkpoint: `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3`.

### Reference device and runtime

- Model Name: MacBook Pro
- Model Identifier: MacBookPro18,2
- Chip: Apple M1 Max
- Total Number of Cores: 10 (8 Performance and 2 Efficiency)
- Memory: 32 GB
- System Version: macOS 26.6.2 (25G83)
- Kernel Version: Darwin 25.6.0
- Secure Virtual Memory: Enabled

Python 3.12.14; torch 2.14.1, transformers 5.20.2, sentence-transformers 6.2.0, numpy 2.5.4, psutil 7.2.1, huggingface-hub 2.3.0, tokenizers 0.23.3, safetensors 0.8.0, pillow 12.0.0, torchvision 0.29.1.
Loaded parameters: 595,776,512; disabled encoders: none.
On AC power with other applications open. Power and thermal snapshots before/after the run are retained in the raw result. This is a desktop screening result, not a controlled battery or native-app responsiveness measurement.

### Retrieval on 35 positive test queries

| Method | Relevant first | Recall@5 | MRR@10 | nDCG@10 |
|---|---:|---:|---:|---:|
| Current lexical behavior proxy | 22.9% | 22.9% | 0.229 | 0.229 |
| BM25 diagnostic | 74.3% | 91.4% | 0.813 | 0.833 |
| Embedding alone | 74.3% | 94.3% | 0.846 | 0.878 |
| BM25 + embedding | 80.0% | 94.3% | 0.869 | 0.889 |

Recall@5 counts all judged relevant passages, including useful related evidence. MRR measures the reciprocal position of the first relevant item; nDCG uses the graded relevance judgments. Small, agent-authored samples are not evidence of general correspondence quality.

| Test category | Queries | Embedding Recall@5 | Embedding relevant first | Hybrid relevant first |
|---|---:|---:|---:|---:|
| cross_language | 4 | 100.0% | 50.0% | 50.0% |
| entities | 1 | 100.0% | 100.0% | 100.0% |
| identifiers | 3 | 66.7% | 33.3% | 66.7% |
| lifecycle | 4 | 100.0% | 75.0% | 100.0% |
| long_passage | 2 | 50.0% | 0.0% | 0.0% |
| mathematics | 3 | 100.0% | 100.0% | 100.0% |
| negation | 3 | 100.0% | 66.7% | 66.7% |
| paraphrase | 6 | 100.0% | 83.3% | 83.3% |
| quoted_phrase | 2 | 100.0% | 100.0% | 100.0% |
| structured_filter | 5 | 100.0% | 100.0% | 100.0% |
| structured_meaning | 2 | 100.0% | 100.0% | 100.0% |

### Material retrieval failures

Every test query for which the embedding's first result was not judged relevant is listed below. These judgments are reviewable in the fixture; no failure was removed from the aggregate.

| Query | Intended target | First embedding result | Target position |
|---|---|---|---:|
| q02: Find the decision to send money back for the damaged headphones, without requiring a return. | d03: Purchase adjustment | d04: Replacement approved | 2 |
| q10: Which message says the workshop was not delayed after all? | d02: Friday planning: no schedule change | d01: Friday planning | 2 |
| q15: Find the Thursday meeting moved upstairs rather than held in a café. | d23: Changement de salle | d26: 会议地点 | 3 |
| q16: Which meeting will start at four instead of three tomorrow, with the location unchanged? | d25: 会議の時間 | d26: 会议地点 | 2 |
| q19: REF-4828 | d04: Replacement approved | d09: Figures for the monthly report | 2 |
| q20: SHIP-2186 | d35: Delay in receiving | d36: Contents damaged | 7 |
| q29: Which review explains that statistical independence cannot be inferred from zero covariance? | d47: Proof review: same conclusion, different assumption | d48: Proof review: independence assumed | 2 |
| q30: Find the final public workshop date after the Monday and Tuesday suggestions were dropped. | d49: A schedule correction in a long conversation | d50: Draft schedule from last week | 8 |
| q39: full refund for broken headphones | d03: Purchase adjustment | d04: Replacement approved | 2 |

The fixture challenges refund versus replacement, a rejected versus agreed delay, precise identifiers, cross-language time/location changes, an implication versus its converse, and a final decision buried in a longer message. The table identifies this model's actual first-result failures. Similarity is evidence of related subject matter, not a factual interpretation or proof that a passage answers the query.

### No-answer queries

| Query | Highest eligible cosine | Top occurrence |
|---|---:|---|
| q43: What is the secret password for the workshop projector? | 0.609 | d08: Please keep this private |
| q44: Find the final train ticket reservation for the speaker. | 0.469 | d37: Draft for the next meeting |
| q45: Which email confirms that the blue violin was sold? | 0.439 | d04: Replacement approved |
| q46: E-ALIAS-999 | 0.564 | d27: Build failure evidence |
| q47: microscope adapter | — | No eligible occurrence |

The model ranks unrelated passages even when the corpus has no answer. No confidence threshold was calibrated using the test set; production absence/uncertainty behavior remains open.

### Measured resource cost

- Cached checkpoint load: 2.70 seconds (separate from package import and download).
- Initial corpus: 54 passages in 1.28 seconds, 42.2 passages/second; input tokens min/median/max 29/58/243.
- Sustained batch-four workload: 128 repetitions of two long fictional passages, 214–243 tokens, 6.37 seconds, 20.1 passages/second. Repetition measures throughput, not retrieval quality or large-corpus scaling.
- Warm single-query encoding: median 32.2 ms, p95 35.8 ms over 48 sequential timings (first 16 fixture queries, three repetitions, MPS synchronized). These times exclude vector-store search, decryption and UI work.
- Sampled process RSS peak 3.67 GiB; OS process maximum 3.73 GiB. Sampled Metal-driver peak 2.65 GiB, active Metal allocation 2.28 GiB.
- Predeclared conservative incremental RSS + Metal-driver sum: 5.92 GiB. Unified-memory accounting can double-count; this is not a measurement of unique physical memory or an app memory budget.
- Battery/energy and foreground app responsiveness: not measured. No quantized, Core ML, MLX, GGUF or mobile runtime was evaluated.

### Frozen acceptance gates

| Gate | Outcome |
|---|---|
| No fixture eligibility/filter leakage | PASS — 0 violations |
| Embedding Recall@5 ≥90% and MRR@10 ≥0.75 | PASS — 94.3%; 0.846 |
| Hybrid exact identifier/phrase hit@1 ≥ lexical proxy | FAIL — hybrid 80.0%, lexical 100.0% |
| Warm query p95 ≤1 second | PASS — 0.036 s |
| Cached load ≤15 seconds | PASS — 2.70 s |
| Long-passage throughput ≥2 passages/second | PASS — 20.1/s |
| Conservative memory sum <6 GiB | PASS — 5.92 GiB (can double-count unified memory) |

Passing the synthetic eligibility gate validates this fixture harness's prefilter, not the production client's ciphertext, concurrency or authorization boundaries. A failed resource or exact-retrieval gate is not waived because an aggregate semantic score passes.

## google/embeddinggemma-2

Pinned checkpoint: `914f7f89142e33e77833254d9c9b90c3cef7303b`.

### Reference device and runtime

- Model Name: MacBook Pro
- Model Identifier: MacBookPro18,2
- Chip: Apple M1 Max
- Total Number of Cores: 10 (8 Performance and 2 Efficiency)
- Memory: 32 GB
- System Version: macOS 26.6.2 (25G83)
- Kernel Version: Darwin 25.6.0
- Secure Virtual Memory: Enabled

Python 3.12.14; torch 2.14.1, transformers 5.20.2, sentence-transformers 6.2.0, numpy 2.5.4, psutil 7.2.1, huggingface-hub 2.3.0, tokenizers 0.23.3, safetensors 0.8.0, pillow 12.0.0, torchvision 0.29.1.
Loaded parameters: 271,002,624; disabled encoders: vision, audio.
On AC power with other applications open. Power and thermal snapshots before/after the run are retained in the raw result. This is a desktop screening result, not a controlled battery or native-app responsiveness measurement.

### Retrieval on 35 positive test queries

| Method | Relevant first | Recall@5 | MRR@10 | nDCG@10 |
|---|---:|---:|---:|---:|
| Current lexical behavior proxy | 22.9% | 22.9% | 0.229 | 0.229 |
| BM25 diagnostic | 74.3% | 91.4% | 0.813 | 0.833 |
| Embedding alone | 74.3% | 100.0% | 0.862 | 0.898 |
| BM25 + embedding | 88.6% | 94.3% | 0.910 | 0.924 |

Recall@5 counts all judged relevant passages, including useful related evidence. MRR measures the reciprocal position of the first relevant item; nDCG uses the graded relevance judgments. Small, agent-authored samples are not evidence of general correspondence quality.

| Test category | Queries | Embedding Recall@5 | Embedding relevant first | Hybrid relevant first |
|---|---:|---:|---:|---:|
| cross_language | 4 | 100.0% | 50.0% | 50.0% |
| entities | 1 | 100.0% | 100.0% | 100.0% |
| identifiers | 3 | 100.0% | 33.3% | 100.0% |
| lifecycle | 4 | 100.0% | 100.0% | 100.0% |
| long_passage | 2 | 100.0% | 50.0% | 100.0% |
| mathematics | 3 | 100.0% | 100.0% | 100.0% |
| negation | 3 | 100.0% | 66.7% | 66.7% |
| paraphrase | 6 | 100.0% | 66.7% | 83.3% |
| quoted_phrase | 2 | 100.0% | 100.0% | 100.0% |
| structured_filter | 5 | 100.0% | 100.0% | 100.0% |
| structured_meaning | 2 | 100.0% | 50.0% | 100.0% |

### Material retrieval failures

Every test query for which the embedding's first result was not judged relevant is listed below. These judgments are reviewable in the fixture; no failure was removed from the aggregate.

| Query | Intended target | First embedding result | Target position |
|---|---|---|---:|
| q02: Find the decision to send money back for the damaged headphones, without requiring a return. | d03: Purchase adjustment | d04: Replacement approved | 2 |
| q06: Which explanation says the value forty-eight counts transactions rather than people? | d09: Figures for the monthly report | d10: Customer measure | 2 |
| q10: Which message says the workshop was not delayed after all? | d02: Friday planning: no schedule change | d49: A schedule correction in a long conversation | 3 |
| q11: What should we do when we cannot tell whether a charge has already happened? | d17: Retry behavior | d18: Definite refusal | 2 |
| q15: Find the Thursday meeting moved upstairs rather than held in a café. | d23: Changement de salle | d24: Treffen im Café | 2 |
| q16: Which meeting will start at four instead of three tomorrow, with the location unchanged? | d25: 会議の時間 | d26: 会议地点 | 2 |
| q19: REF-4828 | d04: Replacement approved | d18: Definite refusal | 3 |
| q20: SHIP-2186 | d35: Delay in receiving | d36: Contents damaged | 2 |
| q30: Find the final public workshop date after the Monday and Tuesday suggestions were dropped. | d49: A schedule correction in a long conversation | d50: Draft schedule from last week | 2 |

The fixture challenges refund versus replacement, a rejected versus agreed delay, precise identifiers, cross-language time/location changes, an implication versus its converse, and a final decision buried in a longer message. The table identifies this model's actual first-result failures. Similarity is evidence of related subject matter, not a factual interpretation or proof that a passage answers the query.

### No-answer queries

| Query | Highest eligible cosine | Top occurrence |
|---|---:|---|
| q43: What is the secret password for the workshop projector? | 0.691 | d08: Please keep this private |
| q44: Find the final train ticket reservation for the speaker. | 0.663 | d49: A schedule correction in a long conversation |
| q45: Which email confirms that the blue violin was sold? | 0.623 | d41: Equipment return update |
| q46: E-ALIAS-999 | 0.736 | d28: Delivery failure evidence |
| q47: microscope adapter | — | No eligible occurrence |

The model ranks unrelated passages even when the corpus has no answer. No confidence threshold was calibrated using the test set; production absence/uncertainty behavior remains open.

### Measured resource cost

- Cached checkpoint load: 2.65 seconds (separate from package import and download).
- Initial corpus: 54 passages in 1.43 seconds, 37.8 passages/second; input tokens min/median/max 34/62/245.
- Sustained batch-four workload: 128 repetitions of two long fictional passages, 217–245 tokens, 3.52 seconds, 36.4 passages/second. Repetition measures throughput, not retrieval quality or large-corpus scaling.
- Warm single-query encoding: median 31.8 ms, p95 42.2 ms over 48 sequential timings (first 16 fixture queries, three repetitions, MPS synchronized). These times exclude vector-store search, decryption and UI work.
- Sampled process RSS peak 2.05 GiB; OS process maximum 2.05 GiB. Sampled Metal-driver peak 2.06 GiB, active Metal allocation 1.10 GiB.
- Predeclared conservative incremental RSS + Metal-driver sum: 3.71 GiB. Unified-memory accounting can double-count; this is not a measurement of unique physical memory or an app memory budget.
- Battery/energy and foreground app responsiveness: not measured. No quantized, Core ML, MLX, GGUF or mobile runtime was evaluated.

### Frozen acceptance gates

| Gate | Outcome |
|---|---|
| No fixture eligibility/filter leakage | PASS — 0 violations |
| Embedding Recall@5 ≥90% and MRR@10 ≥0.75 | PASS — 100.0%; 0.862 |
| Hybrid exact identifier/phrase hit@1 ≥ lexical proxy | PASS — hybrid 100.0%, lexical 100.0% |
| Warm query p95 ≤1 second | PASS — 0.042 s |
| Cached load ≤15 seconds | PASS — 2.65 s |
| Long-passage throughput ≥2 passages/second | PASS — 36.4/s |
| Conservative memory sum <6 GiB | PASS — 3.71 GiB (can double-count unified memory) |

Passing the synthetic eligibility gate validates this fixture harness's prefilter, not the production client's ciphertext, concurrency or authorization boundaries. A failed resource or exact-retrieval gate is not waived because an aggregate semantic score passes.

## Paired uncertainty

Qwen minus Gemma mean test-query MRR difference: -0.016; paired 5,000-resample bootstrap interval [-0.082, 0.047] (seed 20261025). This interval describes these agent-authored queries, not general users or domains.

## Decision and next boundary

Prefer EmbeddingGemma 2 text-only as the candidate for the next integration experiment on this Mac: its sampled resource cost is lower and sustained long-passage throughput higher, while retrieval is competitive in this small screen. The paired MRR interval includes zero, so do not claim a demonstrated general retrieval-quality win. First-result failures still occur, including opposite meanings; a 100% top-five result is not 100% first-result accuracy. This recommendation does not select or ship a production model.

These runs screen the two permissively licensed candidates for local text retrieval. Material ranking failures, the exact-match regression gate, conservative memory accounting and the paired uncertainty interval must govern interpretation; no candidate becomes a production choice merely by passing aggregate recall. The original 300M model was not measured and its provider-specific terms are not being accepted for this comparison.

Before a production choice, review relevance judgments with the owner, expand independent held-out correspondence and hard negatives, evaluate exact-identifier precedence and abstention on a new development/test split, and measure an actual bounded Apple runtime (including quantization parity, cancellation/lock behavior and foreground interaction). Preserve the float32 baseline for conversion checks. Model evaluation does not implement semantic or structured search in the member app.

Official sources: [Qwen model and required query preparation](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B), [EmbeddingGemma 2 checkpoint, text-only loading, prompts and precision](https://huggingface.co/google/embeddinggemma-2), [Google's Apache-2.0 license statement](https://ai.google.dev/gemma/docs/embeddinggemma). These license declarations identify the evaluated artifacts; this report does not attempt a general interpretation of the older Gemma terms.
