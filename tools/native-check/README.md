# Native app checks (macOS)

Scripts that drive the running desktop app (`pnpm -C app tauri dev`,
process `zeroed`) the way a person would, for checks the browser
cannot do: native menus, file dialogs, Finder open, drag and drop.

Needs, for the app that runs them (Terminal, or Claude): Screen
Recording, Accessibility and Automation (System Events, Finder) in System
Settings > Privacy & Security.

- `swiftc -O mouse.swift -o mouse` once: real mouse clicks, drags and
  scrolls (`mouse click x y`, `mouse drag x1 y1 x2 y2`, `mouse scroll x y dy`).
- `nat.sh`: `menu File "Save As..."`, `submenu File Import "..."`,
  `key s "command down"`, `keycode 53`, `click x y` (coordinates in a
  1400 wide window shot), `shot name` (window screenshot), `windows`.
- `menus.applescript`: dumps every menu item with its enabled state.
- `sweep.sh tag "Menu|Item" ...`: clicks items, screenshots each, builds
  `tag_sheet.png`.
- `smoke.sh`: menu sweep, checks every menu event reached the page (reads
  `tauri.log` next to the scripts: start tauri dev with its output there),
  and a Save As / New / Open round trip in `files/`.
