# Changelog

Every release of Zeroed, newest first. The version lives in
`app/package.json` (tauri.conf.json reads it) and the Cargo workspace;
`tools/release/set-version.sh` changes both. Help > About shows it.

## Unreleased

## 0.1.0 (2026-09-28)

The first build that installs on other Macs.

- Design: vector drawing, text, dimensions, transforms, booleans, offsets,
  fillets and arrays; import of SVG, DXF, DWG, EPS, AI, PDF and bitmaps;
  export of SVG, DXF, EPS, AI and PDF.
- Modeling: shapes, sweeps, turned and spun parts, sculpting, component
  import (STL, OBJ, 3DS, 3DM) and STL export.
- Toolpaths: 2D, V-Groove, 3D roughing and finishing, drilling, inlays,
  textures and moldings, a tool database, simulation preview and post
  processors for common controllers.
- Clipart library, gadgets and assembly of the cut parts.
- Shipping: one app version, shown in Help > About.
- Shipping: a .app and a .dmg with Finder document icons for .carve and
  .carveclip; signing and notarization steps (README, Release).
- Crash reports written on the Mac and offered at the next start, with an
  offer to restore the backup of an unsaved job; a fault in a command or a
  damaged file ends as an error instead of taking the app down.
- The window shows unsaved changes (the dot in the close button) and its
  file (Command-click on the title).
