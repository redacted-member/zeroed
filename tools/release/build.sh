#!/bin/zsh
# build.sh: the release .app and .dmg in target/release/bundle (or under
# CARGO_TARGET_DIR). The app is ad-hoc signed (tauri.conf.json,
# signingIdentity "-") and not notarized, so no certificate or Apple account
# is needed. README.md, "Installing on another Mac", says how to open it on
# another Mac.
set -e
root=${0:A:h:h:h}
cd $root/app
# The build step is given explicitly so it also works where pnpm refuses to
# run (a node_modules linked from another checkout).
npx tauri build --bundles app,dmg --config '{"build":{"beforeBuildCommand":"npx vite build"}}'
bundle=${CARGO_TARGET_DIR:-$root/target}/release/bundle
ls -d $bundle/macos/*.app $bundle/dmg/*.dmg
