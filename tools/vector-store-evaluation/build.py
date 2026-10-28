"""Reproduce isolated native builds from pinned official sources, never install them."""
from pathlib import Path
import hashlib
import io
import json
import subprocess
import tarfile
import urllib.request

ROOT=Path(__file__).resolve().parents[2]
WORK=ROOT/'target/vector-store-evaluation'
SOURCES={
 'sqlcipher':('https://codeload.github.com/sqlcipher/sqlcipher/tar.gz/refs/tags/v4.19.0','7075f96cbabe45b4ecfc2e6b1745a625f856f695b0827a5506ce9ed85b906aa0'),
 'sqlite-vec':('https://github.com/asg017/sqlite-vec/releases/download/v0.1.9/sqlite-vec-0.1.9-amalgamation.tar.gz','3acd67cb4aff080c7050926fd3cf8227905fe5b7ee3829d8ee5024ab1283cf61'),
 'vec1':('https://raw.githubusercontent.com/sqlite/sqlite/4012bdca5f1599d5dd3d6671112f170bfd48a26f/ext/vec1/vec1.c','01c91474cce6c1c4872b45e00d8d1934a909baa68ecac679bcdf1093fa054807')}
WORK.mkdir(parents=True,exist_ok=True)
for name,(url,expected) in SOURCES.items():
 data=urllib.request.urlopen(url,timeout=60).read()
 assert hashlib.sha256(data).hexdigest()==expected,'Publisher source changed: '+name
 if name=='vec1':
  (WORK/'vec1.c').write_bytes(data);continue
 with tarfile.open(fileobj=io.BytesIO(data),mode='r:gz') as archive:
  for member in archive.getmembers():
   if not member.isfile():continue
   relative=Path(*Path(member.name).parts[1:]) if name=='sqlcipher' else Path(member.name)
   assert not relative.is_absolute() and '..' not in relative.parts
   destination=WORK/name/relative;destination.parent.mkdir(parents=True,exist_ok=True)
   destination.write_bytes(archive.extractfile(member).read());destination.chmod(member.mode & 0o755)
flags='-O3 -DSQLITE_HAS_CODEC -DSQLITE_EXTRA_INIT=sqlcipher_extra_init -DSQLITE_EXTRA_SHUTDOWN=sqlcipher_extra_shutdown -DSQLITE_TEMP_STORE=3 -DSQLCIPHER_CRYPTO_CC'
commands=[(['./configure','--with-tempstore=always','--enable-fts5','CFLAGS='+flags,'LDFLAGS=-framework Security -framework CoreFoundation'],WORK/'sqlcipher'),
 (['make','-j4','libsqlite3.dylib','sqlite3'],WORK/'sqlcipher'),
 (['clang','-O3','-DNDEBUG','-dynamiclib','-I'+str(WORK/'sqlcipher'),str(WORK/'vec1.c'),'-o',str(WORK/'vec1.dylib')],ROOT),
 (['clang','-O3','-DNDEBUG','-DSQLITE_VEC_OMIT_FS','-DSQLITE_VEC_ENABLE_NEON','-dynamiclib','-I'+str(WORK/'sqlcipher'),str(WORK/'sqlite-vec/sqlite-vec.c'),'-o',str(WORK/'vec0.dylib')],ROOT)]
with (WORK/'native-build.log').open('w') as output:
 for command,cwd in commands:subprocess.run(command,cwd=cwd,stdout=output,stderr=subprocess.STDOUT,check=True)
(WORK/'build-recipe.json').write_text(json.dumps({'sources':SOURCES,'commands':[(cmd,str(cwd.relative_to(ROOT))) for cmd,cwd in commands]},indent=2)+'\n')
print('Built isolated SQLCipher, vec1 and sqlite-vec in',WORK)
