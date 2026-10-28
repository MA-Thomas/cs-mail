"""Evaluation-only contracts. No access to member application stores or credentials."""
from __future__ import annotations

import hashlib
import json
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "target/embedding-evaluation"
FIXTURE = Path(__file__).with_name("fixture.json")
MODELS = {
    "qwen": ("Qwen/Qwen3-Embedding-0.6B", "97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3"),
    "gemma2": ("google/embeddinggemma-2", "914f7f89142e33e77833254d9c9b90c3cef7303b"),
}
QWEN_PROMPT = "Instruct: Given a query about private correspondence, retrieve relevant message passages.\nQuery: "
FUSION_K = 60


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + "\n")


def fixture():
    raw = json.loads(FIXTURE.read_text())
    docs = [{**raw["defaults"], **doc} for doc in raw["documents"]]
    ids = {doc["id"] for doc in docs}
    assert len(ids) == len(docs)
    assert len({q["id"] for q in raw["queries"]}) == len(raw["queries"])
    for query in raw["queries"]:
        assert query["split"] in ("dev", "test")
        assert set(query["relevant"]) <= ids
        assert all(grade in (1, 2) for grade in query["relevant"].values())
        permitted = {doc["id"] for doc in docs if eligible(doc, query, raw["now"])}
        assert set(query["relevant"]) <= permitted
    return raw, docs


def eligible(doc, query, now):
    if doc["account"] != "reference" or not doc["retained"] or doc["removed"] or doc["expires_at"] <= now:
        return False
    for key, value in query.get("filters", {}).items():
        if key == "after":
            if doc["received_at"] <= value:
                return False
        elif key == "before":
            if doc["received_at"] >= value:
                return False
        elif key in ("alias", "sender", "kind"):
            if doc[key] != value:
                return False
        else:
            raise ValueError("Unknown fixture filter: " + key)
    return True


def words(text):
    # Matches the current client's Unicode alphanumeric boundaries for this fixture.
    return re.findall(r"[^\W_]+", text.lower(), re.UNICODE)


def phrase(query):
    text = query["text"].strip()
    return text[1:-1].lower() if len(text) >= 2 and text.startswith('"') and text.endswith('"') else None


def source_text(doc):
    return doc["title"] + "\n\n" + doc["text"]


def lexical_rank(docs, query):
    required = set(words(query["text"]))
    exact = phrase(query)
    return [doc["id"] for doc in sorted(docs, key=lambda d: d["id"])
            if required and required <= set(words(source_text(doc)))
            and (exact is None or exact in source_text(doc).lower())]


def bm25_rank(docs, query):
    if not docs:
        return []
    tokens = [words(source_text(doc)) for doc in docs]
    average = sum(map(len, tokens)) / len(tokens)
    scores = [0.0] * len(docs)
    for term in set(words(query["text"])):
        df = sum(term in row for row in tokens)
        idf = math.log(1 + (len(docs) - df + 0.5) / (df + 0.5))
        for i, row in enumerate(tokens):
            count = row.count(term)
            scores[i] += idf * count * 2.2 / (count + 1.2 * (0.25 + 0.75 * len(row) / average))
    return [docs[i]["id"] for i in sorted(range(len(docs)), key=lambda i: (-scores[i], docs[i]["id"])) if scores[i] > 0]


def fused_rank(lexical, semantic):
    score = {}
    for ranking in (lexical[:50], semantic[:50]):
        for rank, identifier in enumerate(ranking, start=1):
            score[identifier] = score.get(identifier, 0) + 1 / (FUSION_K + rank)
    return sorted(score, key=lambda identifier: (-score[identifier], identifier))


def metrics(ranking, relevant):
    if not relevant:
        return None
    gains = [relevant.get(identifier, 0) for identifier in ranking[:10]]
    dcg = sum((2 ** grade - 1) / math.log2(rank + 2) for rank, grade in enumerate(gains))
    ideal = sum((2 ** grade - 1) / math.log2(rank + 2)
                for rank, grade in enumerate(sorted(relevant.values(), reverse=True)[:10]))
    return {"hit1": float(bool(ranking and ranking[0] in relevant)),
            "recall5": sum(identifier in relevant for identifier in ranking[:5]) / len(relevant),
            "mrr10": next((1 / (rank + 1) for rank, grade in enumerate(gains) if grade), 0),
            "ndcg10": dcg / ideal}


def freeze_inputs():
    paths = [Path(__file__).with_name(name) for name in ("fixture.json", "README.md", "common.py", "evaluate.py", "requirements.txt")]
    return {str(path.relative_to(ROOT)): digest(path) for path in paths}


def check_frozen():
    frozen = json.loads((WORK / "frozen.json").read_text())
    assert frozen["inputs"] == freeze_inputs(), "Evaluation inputs changed after freeze"
    return frozen
