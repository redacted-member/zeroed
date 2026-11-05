#!/bin/zsh
# set-version.sh <x.y.z>: sets the one app version everywhere it is written:
# the Cargo workspace (Cargo.toml, Cargo.lock) and app/package.json, which
# tauri.conf.json reads. Add a "## x.y.z (date)" section to CHANGELOG.md too;
# app/src/lib/appVersion.test.ts fails until every place agrees.
set -e
v=${1:?usage: set-version.sh <major.minor.patch>}
[[ $v =~ '^[0-9]+\.[0-9]+\.[0-9]+$' ]] || { echo "not a version: $v" >&2; exit 1; }
root=${0:A:h:h:h}
cd $root
python3 - "$v" <<'PY'
import json, re, sys
v = sys.argv[1]
text = open('Cargo.toml').read()
head, sep, rest = text.partition('[workspace.package]')
rest = re.sub(r'^version = "[^"]+"$', f'version = "{v}"', rest, count=1, flags=re.M)
open('Cargo.toml', 'w').write(head + sep + rest)
path = 'app/package.json'
text = open(path).read()
text = re.sub(r'^(  "version": )"[^"]+"', rf'\g<1>"{v}"', text, count=1, flags=re.M)
open(path, 'w').write(text)
PY
cargo update --workspace --offline -q
grep -q "^## $v " CHANGELOG.md || echo "CHANGELOG.md has no section for $v yet: add one"
echo "version $v set"
