"""Download only pinned publisher artifacts; never accept terms or log a token."""
import argparse
import os
from common import MODELS, WORK, write_json

os.environ["HF_HOME"] = str(WORK / "huggingface")
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
os.environ["DO_NOT_TRACK"] = "1"

parser = argparse.ArgumentParser()
parser.add_argument("--model", choices=MODELS, required=True)
args = parser.parse_args()
from huggingface_hub import snapshot_download
from huggingface_hub.errors import GatedRepoError

model, revision = MODELS[args.model]
try:
    path = snapshot_download(model, revision=revision, token=False, max_workers=2,
                             allow_patterns=["*.json", "*.safetensors", "*.txt", "*.model", "*.jinja", "merges.txt", "README.md", "LICENSE"])
except GatedRepoError:
    write_json(WORK / (args.model + "-access.json"), {"model": model, "revision": revision, "status": "blocked_official_checkpoint_access"})
    raise SystemExit("Official checkpoint access is gated. Owner-authorized local access is required; no model was substituted.") from None
print("Downloaded official checkpoint:", path)
