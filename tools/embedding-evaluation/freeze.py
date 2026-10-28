"""Freeze the evaluation protocol and fixture before any model result is observed."""
from datetime import datetime, timezone
from common import WORK, fixture, freeze_inputs, write_json

raw, docs = fixture()
path = WORK / "frozen.json"
assert not path.exists(), "A frozen evaluation already exists; do not silently replace it"
write_json(path, {"frozen_at_utc": datetime.now(timezone.utc).isoformat(), "inputs": freeze_inputs(),
                  "documents": len(docs), "queries": len(raw["queries"]),
                  "test_queries": sum(q["split"] == "test" for q in raw["queries"])})
print("Frozen evaluation inputs:", path)
