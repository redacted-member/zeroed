#!/bin/zsh
# gate.sh <worktree>: the full gate, with the cargo test build and run split so the binaries
# run are the ones built from this worktree (the shared target is rebuilt by
# other worktrees): touch, build (the log shows Compiling from this path),
# then run exactly those executables. Stops on the first failed step.
set -e
W=${1:?worktree}; cd $W
R=$(git -C $W rev-parse --path-format=absolute --git-common-dir)/..
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$R/target}
# A cfg of its own gives this gate's artifacts their own hashes in the shared
# target, so another worktree's build of the same crates cannot stand in for
# this one (their fingerprints name that worktree's files).
tag=carve_gate_${${W:t}//[^A-Za-z0-9]/_}
export RUSTFLAGS="--cfg $tag" RUSTDOCFLAGS="--cfg $tag"
touch crates/carve-core/src/lib.rs app/src-tauri/src/main.rs crates/carve-devhost/src/main.rs
echo "== fmt"; cargo fmt --all --check
echo "== clippy"; cargo clippy -q -p carve-core -p carve-devhost --all-targets -- -D warnings
echo "== cargo test"
p=0
for pkg in carve-core carve-devhost; do
  touch crates/carve-core/src/lib.rs crates/carve-devhost/src/main.rs
  build=$(cargo test --no-run -p $pkg 2>&1) || { echo "$build" | tail -30; exit 1; }
  echo "$build" | grep -qE "Compiling $pkg v.*\($W/" || { echo "$pkg not rebuilt from $W"; echo "$build" | grep Compiling; exit 1; }
  exes=(${(f)"$(echo "$build" | sed -nE 's/.*Executable .*\((.*)\)$/\1/p')"})
  for exe in $exes; do
    out=$(cd crates/$pkg && $exe 2>&1) || { echo "$out" | grep -vE "^test .* ok$" | tail -40; exit 1; }
    r=$(echo "$out" | grep -E "^test result" | tail -1)
    echo "${exe:t}: $r"
    p=$((p + $(echo "$r" | awk '{print $4}')))
  done
  if [ $pkg = carve-core ]; then
    doc=$(cargo test -q --doc -p $pkg 2>&1) || { echo "$doc" | tail -30; exit 1; }
    echo "doc $pkg: $(echo "$doc" | grep -E "^test result" | tail -1)"
  fi
done
echo "cargo passed $p failed 0"
cd app
echo "== tsc"; npx tsc --noEmit; npx tsc --noEmit -p scripts
echo "== eslint"; npx eslint .
echo "== prettier"; npx prettier --check . >/dev/null
echo "== vitest"; v=$(npx vitest run 2>&1) || { echo "$v" | grep -E "FAIL|Tests " ; exit 1; }; echo "$v" | grep -E "Tests "
echo "== build"; npx vite build 2>&1 | grep -E "index-.*js|error|budget"
echo GATE PASSED
