# Developing Zeroed

## Layout

```
Cargo.toml              Rust workspace
crates/carve-core/      the kernel: geometry, model, CAM, post, document, io
app/                    front end (Vite, React, TypeScript, Sass)
  src/bridge/           typed kernel client, plus an in-memory mock
  src/shell/            app shell, panels, tab bar, status bar
  src/views/            2D canvas view and 3D three.js view
  src/forms/            one folder per form, with its pure form model
  src/state/            zustand stores mirroring the kernel
  src/styles/           design tokens and the global stylesheet
  src-tauri/            desktop shell: window, native menu, kernel host
docs/                   kernel API, machine checklist, test projects, licenses
```

## Prerequisites

- Rust 1.82 or newer, from Homebrew (`brew install rust`) or rustup.
- Node 22 or newer.
- pnpm 10 or newer (`npm install -g pnpm`).
- Xcode command line tools (`xcode-select --install`), needed by the macOS
  build of the shell.

No API keys, no environment variables and no Apple developer account are
needed to build, run or release.

## Run

```
pnpm -C app install          # once
pnpm -C app tauri dev        # the desktop app
pnpm -C app dev              # the interface in a browser, real kernel
```

`pnpm -C app dev` builds the kernel dev host in release mode, starts it on
port 1421 and serves the interface on http://localhost:1420, proxying
`/kernel` to it. Set `VITE_PORT` and `CARVE_DEVHOST_PORT` to run a second
copy beside the first. Restart it after changing kernel code. The dev host
uses your real tool database, so tidy up any test tools you add. The native
menu only exists in the desktop app.

## Test and check

```
cargo test --workspace                    # kernel and shell tests
cargo clippy --workspace -- -D warnings   # Rust lints
cargo fmt --all --check                   # Rust formatting
pnpm -C app test                          # front end unit tests
pnpm -C app typecheck                     # TypeScript
pnpm -C app lint                          # ESLint and Prettier
```

## Build

```
pnpm -C app tauri build --debug   # debug .app for local use
tools/release/build.sh            # release .app and .dmg
```

The bundles land in `target/<profile>/bundle/macos/Zeroed.app` and
`target/release/bundle/dmg/Zeroed_<version>_aarch64.dmg` (under
`CARGO_TARGET_DIR` when it is set). The app is ad-hoc signed and not
notarized; see Installing on another Mac.

The bundle declares the file types it opens (`bundle.fileAssociations` in
`app/src-tauri/tauri.conf.json`). `app/src-tauri/Info.plist` repeats that list
with the Finder document icons for `.carve` and `.carveclip`, which the
configuration has no field for, and replaces the generated list in the
bundle; `app/src/lib/bundleTypes.test.ts` fails when the two differ.
`tools/release/document-icons.py` redraws the document icons.

## Release

The version is kept in `app/package.json` (tauri.conf.json reads it) and in
the Cargo workspace; Help > About shows it. For a new version:

1. `tools/release/set-version.sh 0.2.0` and a `## 0.2.0 (date)` section in
   `CHANGELOG.md`.
2. `tools/release/build.sh`, then hand out the `.dmg`.

The build is for Apple silicon only. An Intel or universal build needs
rustup with the `x86_64-apple-darwin` target and
`tools/release/build.sh` run with `--target universal-apple-darwin` added to
its `tauri build` line.

There is no update check. Adding one would take the Tauri updater plugin, an
update signing key pair (the private key kept outside the repository, the
public key in tauri.conf.json), a place to host the signed update archives
and a `latest.json` describing them, and a Help menu item or start check that
asks before installing.

### Installing on another Mac

The app is ad-hoc signed and not notarized, so no Apple developer account
or certificate is involved. On the Mac that built it, it just opens. On
another Mac, copy `Zeroed.app` to `/Applications` (from the `.dmg`);
the first open is then blocked. Either open it once, then go to System
Settings > Privacy & Security and click Open Anyway, or run:

```
xattr -dr com.apple.quarantine /Applications/Zeroed.app
```

Building from source on that Mac avoids the block.

## Crash reports

When the app panics it writes a report (version, time, message and stack)
to `Crash Reports` in the application data folder
(`~/Library/Application Support/Zeroed`), and nothing leaves the Mac.
The next start after a crash offers to show the report, then to restore the
autosave backup of a job that had unsaved changes (Restore, Open Later,
Discard). Each running copy keeps a small record in `Sessions` there; a clean
quit removes it, so a record whose process is gone marks a crash. A kernel
command that panics answers with an error instead of taking the app down.

## How the pieces talk

The kernel runs inside the shell process. The front end calls one Tauri
command, `kernel_call(name, args)`, and gets back four ASCII tag bytes then a
payload: `JSON` for a reply envelope, `BLOB` for a raw buffer. A reply is
either `{"ok": true, "result": ...}` or `{"ok": false, "error": {"code",
"message"}}`, and every command that changes the document also reports the new
document revision. [`docs/KERNEL_API.md`](KERNEL_API.md) is the contract, the typed
client lives in `app/src/bridge/`.

Native menu items emit `menu://<id>` events, for example `menu://edit.jobSetup`.
The shell subscribes to the ids it handles in `app/src/lib/menu.ts`.

## Post processors and drill banks

The shipped post processors live in `crates/carve-core/assets/posts/`;
Post-Processor Management (Machine menu) imports your own and makes an
editable copy of a shipped one. Posts read the drill bank sections the
post format defines, so a post written for it works unchanged.
A Drill Bank toolpath needs a post with `SETUP_DRILL_BANK` and
`FINAL_DRILL_BANK_ACTION` sections plus one way of selecting drills: either
`DRILL_BANK_PATTERN` (the whole set at once) or both
`DRILL_BANK_EXTEND_DRILL` and `DRILL_BANK_RETRACT_DRILL` (one drill at a
time). Posts with neither, or with both, refuse it.

For each drill bank toolpath the setup section is written once before its
first move and the final section once after its last. At each firing, after
the rapid at Safe Z that brings the reference drill over its hole, a pattern
post writes `DRILL_BANK_PATTERN` when the set of drills differs from the set
already down; a per drill post writes a retract for each drill down that is
not wanted, then an extend for each wanted drill not yet down, and raises
every drill still down before the final section. The sections can use
`[DB]` (the drills down as a bit pattern: drill n is worth 2 to the power
n - 1), `[DBX]` and `[DBY]` (the same for the horizontal arm, the central
drill's row, and the vertical arm, its column), `[DL]` (the drills as a list,
shaped by `VAR DRILL_LIST = [DL|prefix|separator|first separator]`) and
`[CD]` (the one drill being lowered or raised). The numbers are the drill
numbers typed in the drill bank template.

The shipped "Generic G code with drill bank (mm)" post
(`drill_bank_mm.pp`) is a pattern post whose sections write only comments,
such as `(DRILLS 1,2,5 PATTERN 19)`. To set up a machine, make a copy of the
post, replace those lines with the codes your controller uses to switch the
bank on, select drills and switch it off, and number the drills in the
template as the machine numbers them. The comments at the top of the post
file describe the sections in full.

### Tool changes on LinuxCNC and UCCNC

The LinuxCNC (mm and inch) and UCCNC posts come in two forms; pick the one
that matches how the machine measures a new tool, in Machine Configuration's
post list or in Save Toolpaths:

- "M6 probes tool length" (`linuxcnc_mm`, `linuxcnc_inch`, `uccnc_mm`, the
  default): for a hand change where M6 runs a probing routine. At each
  change the program stops the spindle, lifts to the clearance height (the
  higher of the safe Z and the home Z), shows a message naming the tool,
  runs `T<n> M6`, lifts straight up to the clearance height again, starts
  the spindle, and only then moves across in X and Y. It writes no length
  offset and no pause: the probing routine applies the length itself.

  LinuxCNC:

  ```
  M5
  G0Z25.0000
  (MSG,Change to tool 2: Drill 6 mm, M6 probes its length)
  T2M6
  G0Z25.0000
  S3000M3
  ```

  UCCNC:

  ```
  N190M5
  N200G0Z25.000
  (Change to tool T2: Drill 6 mm, M6 probes its length)
  N210T2M6
  N220G0Z25.000
  N230S3000M3
  ```

- "manual touch off at M0" (`linuxcnc_manual_mm`, `linuxcnc_manual_inch`,
  `uccnc_manual_mm`): for a hand change with no probe. After `T<n> M6`
  (and `G43 H<n>` on LinuxCNC, which applies the tool table's length) the
  program stops at `M0` while you fit the tool and touch it off, then lifts
  to the clearance height before the spindle starts.

Both forms start with the modal states the moves rely on, before the first
tool change, because LinuxCNC and UCCNC keep what the last program (or the
MDI) left in force: absolute distances (`G90`), feed in units per minute
(`G94`), the units, the XY plane (`G17`), cutter compensation off (`G40`)
and path blending (`G64`). LinuxCNC's `G64 P` is the blending tolerance;
UCCNC reads the `P` of its `G64` as a different setting (the unify length),
so the UCCNC posts write a bare `G64`, which takes the blending settings from
its Settings screen.

LinuxCNC (M6 probes, mm):

```
G90
G94
G21
G17
G40
G64P0.01
M5
G0Z25.0000
(MSG,Change to tool 1: End Mill 6 mm, M6 probes its length)
T1M6
```

UCCNC (M6 probes):

```
N10G90G94
N20G21
N30G17
N40G40
N50G64
N60M5
N70G0Z25.000
(Change to tool T1: End Mill 6 mm, M6 probes its length)
N80T1M6
```

The probing forms never write `G49` (cancel the tool length offset). The
length in force is the one the routine probed for the tool in the spindle;
`G49` would cancel it, and a routine that returns early when asked for the
tool already fitted (many do, to skip a needless probe) would leave the
first toolpath running with no length at all. The manual forms write `G49`
after `G40`, so no old length adds to the `G43 H` or the touch off that
follows.

How the probing routines apply the length, from the controllers' own
documentation and example configurations:

- LinuxCNC: M6 is remapped (`REMAP=M6 ... ngc=<routine>` in the INI file) to
  an O-word routine that waits for the change, probes the tool setter and
  applies the result before it returns. The example shipped with LinuxCNC
  (`configs/sim/axis/remap/manual-toolchange-with-tool-length-switch`)
  applies it as a dynamic offset with `G43.1`, measured against a reference
  tool; the QtDragon routine (`qt_auto_probe_tool.ngc`) writes it into the
  tool table with `G10 L1` and turns it on with `G43`. Either way the offset
  is in force when the program continues, which is why the post writes no
  `G43 H`: after a `G43.1` routine it would swap the probed length for the
  tool table's old one.
- UCCNC: with the M6 setting on "run the M6 macro", `M6` runs the profile's
  `M6.txt` macro. The usual probing macros (with an `M31` macro that
  measures the fixed sensor against the material once per job) move to the
  change position, wait for the new tool, probe it on the fixed sensor and
  set the Z work zero for it. The length lives in the work offset, so the
  post writes no `G43 H`, which would add a tool table length on top.

If your routine does not apply the length itself (for example a remap that
only records it in the tool table), use the manual form or copy the post and
add `G43H[T]` after the change. Air cut the first tool change of any new
setup (see `docs/testing/cut-checklist.md`).

## Bundled fonts

The kernel bundles Fira Sans (SIL Open Font License) as its fallback font and
two single line engraving fonts converted from the public domain Hershey font
data. Sources, licenses and acknowledgements are in
`crates/carve-core/assets/fonts/README.md`.

## Clipart library

The clipart that ships with the app is made by the kernel's own code
(`crates/carve-core/src/clipart/library/`): no third party art is bundled.
Each item is listed with its size, cell size, source file and license in
`crates/carve-core/assets/clipart/MANIFEST.json`; how the library is made,
the quality rules it meets and the outside sources that were checked are in
`crates/carve-core/assets/clipart/README.md`. Items are written into the
cache folder the first time they are drawn or inserted, so the library adds
no files to the app bundle.

## Conventions

Sass per component, TypeScript, no utility CSS frameworks, no UI kits and no inline styles. Every UI string lives in `app/src/lib/strings`. On-screen dimensions are US units by default while the kernel works in millimeters. `tools/agents/gate.sh $PWD` runs the whole check before a change goes in.
