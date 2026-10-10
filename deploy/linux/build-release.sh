#!/usr/bin/env bash
# Run on the target Linux architecture with sibling identity-model checked out.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ $# != 1 || "$(uname -s)" != Linux ]]; then
    echo "usage (on Linux): deploy/linux/build-release.sh /absolute/new-release-directory" >&2
    exit 2
fi
case "$1" in /*) ;; *) echo "output must be absolute" >&2; exit 2 ;; esac
# Never replace an existing release, even if a previous build failed.
umask 022
mkdir "$1"
out="$1"
cd "$root"
sha256sum --check identity-source.sha256 > "$out/identity-check.txt"
host="$(rustc -vV | sed -n 's/^host: //p')"
cargo build --locked --release --no-default-features --target "$host" --target-dir "$root/target" -p cs-maild -p cs-mail-admin
mkdir "$out/bin" "$out/deploy"
install -m 0755 "target/$host/release/cs-maild" "target/$host/release/cs-mail-admin" "$out/bin/"
cp deploy/linux/{cs-mail.service,maild.json.example,callbacks.nginx.conf.example,database.py,README.md} "$out/deploy/"
{
    git rev-parse HEAD
    git status --porcelain
    rustc --version
    cargo --version
    uname -sm
} > "$out/build-context.txt"
# Includes uncommitted Rust, schema and build sources; the Git revision alone is
# insufficient provenance for a development checkout. No private config is copied.
python3 - "$root" "$out/source-sha256.txt" <<'PY'
import hashlib, pathlib, subprocess, sys
root, output = map(pathlib.Path, sys.argv[1:])
with output.open('w') as dst:
    for base in [root, root.parent / 'identity-model']:
        paths = subprocess.check_output(['git', '-C', str(base), 'ls-files', '-z', '--cached', '--others', '--exclude-standard'])
        for name in sorted(set(paths.decode().split('\0')) - {''}):
            path = base / name
            if path.is_file() and path.suffix in {'.rs', '.sql', '.toml', '.lock', '.sh', '.py'}:
                dst.write(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {base.name}/{name}\n')
PY
(cd "$out" && sha256sum bin/* deploy/* build-context.txt identity-check.txt source-sha256.txt > SHA256SUMS)
echo "Release prepared: $out (verify SHA256SUMS after transfer)"
