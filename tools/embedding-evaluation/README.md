# Local embedding comparison

Development evidence only. This is not another member app, a production search
adapter, or a route into the owner's retained mail. All correspondence in the
fixture is fictional. Inputs and vectors stay local; model downloads use the
official publishers. No remote model code is trusted or executed.

## Frozen protocol (before inference)

Reference device: Marcus's available MacBookPro18,2, Apple M1 Max, 32 GB. This
supersedes the proposed newest-Mac reference for this evaluation. Record OS,
power/thermal state, artifact hashes, runtime versions and precision with results.
iPhone performance is outside this run.

Compare Qwen/Qwen3-Embedding-0.6B at revision
`97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` with
google/embeddinggemma-2 at revision
`914f7f89142e33e77833254d9c9b90c3cef7303b`.
Start with 512-dimensional float32, truncated then L2-normalized, using each
checkpoint's supplied pooling/dense modules and retrieval instructions. The first
comparison uses PyTorch MPS float32, batch four, one model process at a time.
EmbeddingGemma 2 runs text-only with vision/audio encoders disabled via the publisher's
configuration (approximately 270M active parameters). Do not substitute third-party
converted weights. Pin and record
the actual checkpoint; fail on truncation, incompatible dimensions or nonfinite vectors.

`fixture.json` supplies manually authored development/test query splits, graded
relevance (2 = direct target, 1 = useful related evidence), and synthetic eligibility
and filter cases. Freeze its SHA-256 before the first result and do not tune it or
the instructions/fusion against test outcomes. These judgments are agent-authored,
not independent user relevance judgments. Report each category and individual
failures; small synthetic data cannot justify a final production-model choice.

Amendment 2, authorized by the owner on 25 October: replace the access-gated
EmbeddingGemma-300M candidate with Apache-2.0 EmbeddingGemma 2. The initial Qwen
pilot was already observed. Its frozen inputs/results are identified separately
under `results/pilot-v1/`. Keep the fixture, queries, judgments, thresholds and
fusion unchanged; rerun Qwen and Gemma 2 with the same updated runtime required
by the new architecture. This is a candidate/runtime amendment, not a claim that
the Qwen outcomes are unseen. Retain pilot metadata until the comparison is reviewed;
its superseded vectors and runtime packages need not remain once replaced.
The unified Gemma 2 processor requires Pillow and Torchvision even with its model's
vision/audio encoders disabled. These prerequisites were established by load-only
preflight before the final two runs, and are pinned in `requirements.txt`. Both
candidates run in that completed environment; no retrieval settings change.

Compare a behavioral proxy of current lexical candidate selection (all Unicode
alphanumeric terms required, quoted phrase checked, deterministic source-ID order),
a BM25 diagnostic, each embedding model alone, and BM25/embedding reciprocal-rank
fusion (constant 60, equal weights, top 50 from each). The proxy does not execute
the real encrypted member-client boundary or reproduce UI ordering. Exact phrase
queries remain explicit constraints for hybrid retrieval. Rank only fixtures that
pass account, retention, removal, expiry and the query's structured filters.
Do not confuse this fixture filter with implemented structured search in the app.

Provisional acceptance gates, declared before measurement:

- Zero ineligible or wrong-filter occurrences in every returned result.
- Held-out semantic Recall@5 at least 0.90 and MRR@10 at least 0.75, with
  per-category failures disclosed rather than hidden by averages.
- Hybrid exact-identifier/quoted-phrase hit@1 no worse than the lexical proxy.
- Warm query-embedding p95 at most 1 second, cached model load at most 15 seconds,
  document throughput at least 2 passages/second and incremental sampled process
  RSS plus Metal-driver allocation below 6 GiB. The memory sum is conservative
  and can double-count unified memory; it is not system peak-memory measurement.

These are exploratory targets, not a product SLO or grounds to bypass privacy or
license requirements. Do not tune them after seeing held-out scores. No-answer
queries have no relevant source: report their top scores, but don't invent a
confidence/abstention threshold from the held-out set. Embeddings always ranking
something does not establish an answer exists.

Measure cached-load time separately from download/import, warm single-query timings
(synchronize MPS), bounded document indexing, process CPU time/RSS and Metal memory.
Save float32 vectors only for this fictional corpus, plus rankings and metadata.
Report on AC power, with other applications running; do not claim controlled energy,
battery-life, native UI responsiveness, encryption-at-rest performance, OCR quality,
production-scale indexing or mobile evidence. Quantized/packaged Apple runtimes need
a later quality-parity and resource evaluation against this float32 baseline.

## Run

The isolated environment lives under `target/embedding-evaluation/venv`. Install
`requirements.txt`, then download the pinned official snapshots with `download.py`.
Both checkpoints are publicly downloadable under Apache 2.0. No Hugging Face
login or acceptance of the older Gemma terms is required for these artifacts.

Run `evaluate.py --model qwen` and `evaluate.py --model gemma2`, then
`report.py`. Evaluation sets offline mode before importing the model runtime.
Record the fixture hash with `freeze.py` before either model run. A changed fixture
requires an explicitly new evaluation, not overwriting the current results.

The final report and compact per-query results are retained as current model-choice
evidence. The downloaded checkpoints, vectors and isolated environment are temporary
evaluation inputs: remove them when the model/runtime selection is resolved or this
comparison is superseded. Do not copy evaluation adapters into production. A chosen
model belongs behind the existing Rust application/endpoint contracts.

References: [Qwen model card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B),
[EmbeddingGemma 2 model card](https://huggingface.co/google/embeddinggemma-2).
