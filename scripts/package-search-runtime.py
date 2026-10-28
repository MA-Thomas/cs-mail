"""Package the pinned offline Mac runtime; no owner data or credentials are accessed."""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(); parser.add_argument('--signer',required=True); args=parser.parse_args()
destination=ROOT/'target/apple-search-runtime'
model=ROOT/'target/embedding-evaluation/huggingface/hub/models--google--embeddinggemma-2/snapshots/914f7f89142e33e77833254d9c9b90c3cef7303b'
venv=ROOT/'target/embedding-evaluation/venv'
base=Path(subprocess.check_output([str(venv/'bin/python'),'-c','import sys;print(sys.base_prefix)'],text=True).strip())
def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for part in iter(lambda:f.read(1024*1024),b''):h.update(part)
    return h.hexdigest()
baseline=json.loads((ROOT/'target/embedding-evaluation/gemma2.json').read_text())
for name,expected in baseline['artifacts_sha256'].items(): assert sha(model/name)==expected
patterns=shutil.ignore_patterns('__pycache__','*.pyc','test','tests','pkgconfig','share')
def ignore(directory,names):
    return set(patterns(directory,names))|{name for name in names if (Path(directory)/name).is_symlink() and not (Path(directory)/name).exists()}
previous=json.loads((destination/'manifest.json').read_text())['files'] if (destination/'manifest.json').exists() else {}
if not (destination/'python/.prepared').exists():
    destination.mkdir(parents=True,exist_ok=True)
    if (destination/'python').exists():shutil.rmtree(destination/'python')
    shutil.copytree(base,destination/'python',ignore=ignore)
    packages=destination/'python/lib/python3.12/site-packages'
    if packages.exists(): shutil.rmtree(packages)
    shutil.copytree(venv/'lib/python3.12/site-packages',packages,ignore=ignore)
    (destination/'python/.prepared').touch()
if not (destination/'model').exists():shutil.copytree(model,destination/'model',symlinks=False)
(destination/'native').mkdir(exist_ok=True)
for name in ('helper.py','sqlcipher.py'):shutil.copyfile(ROOT/'crates/cs-mail-client/runtime'/name,destination/name)
for source,target in [('sqlcipher/libsqlite3.dylib','libsqlite3.dylib'),('vec1.dylib','vec1.dylib')]:
    shutil.copyfile(ROOT/'target/vector-store-evaluation'/source,destination/'native'/target)
# Every bundled native component uses the same stable development signer. No
# original credential or parent app requirement is changed by signing resources.
magic={b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xca\xfe\xba\xbe',b'\xbe\xba\xfe\xca'}
for path in destination.rglob('*'):
    if not path.is_file() or path.is_symlink():continue
    with path.open('rb') as f:first=f.read(4)
    if first in magic:
        verified=subprocess.run(['codesign','--verify','--strict',str(path)],capture_output=True)
        authority=subprocess.run(['codesign','-dv',str(path)],capture_output=True,text=True).stderr
        unchanged=previous.get(str(path.relative_to(destination)))==sha(path)
        if verified.returncode or not unchanged:
            subprocess.run(['codesign','--force','--sign',args.signer,str(path)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
files={str(p.relative_to(destination)):sha(p) for p in sorted(destination.rglob('*')) if p.is_file() and not p.is_symlink() and p!=destination/'manifest.json'}
def fingerprint(value):return list(hashlib.sha256(value.encode()).digest())
profile={'weights':list(bytes.fromhex(files['model/model.safetensors'])),'tokenizer':list(bytes.fromhex(files['model/tokenizer.json'])),
         'preprocessing':fingerprint('cs-mail/canonical-reading-parts/v1;subject-context-first128tokens;no-reference-following'),
         'passage_rules':fingerprint('body-token-offsets384;overlap64;max4-per-turn;utf8-range;v1'),
         'instructions':fingerprint('title: {subject} | text: {passage};query=task: search result | query: '),
         'runtime_precision':fingerprint(json.dumps({'runtime':baseline['packages'],'helper':files['helper.py'],'python':sha(base/'bin/python3.12'),'storage':sha(ROOT/'target/vector-store-evaluation/sqlcipher/libsqlite3.dylib'),'vec1':sha(ROOT/'target/vector-store-evaluation/vec1.dylib'),'dtype':'mps-float32-truncate512-l2'},sort_keys=True)),
         'dimensions':512,'normalized':True}
(destination/'manifest.json').write_text(json.dumps({'files':files,'profile':profile},indent=2)+'\n')
print('Packaged pinned local search resources:',destination)
