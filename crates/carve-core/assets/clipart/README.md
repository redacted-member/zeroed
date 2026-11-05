# Clipart library

## What ships

Most items in the library are made by our own code in
`crates/carve-core/src/clipart/library/`: reliefs from formulas and from the
kernel's modeling functions (Create Shape, distance based molded edges, round
tubes along knot curves), and vectors from geometry and from outlines drawn
point by point for this library. Those carry the project's own license (MIT,
see the workspace `Cargo.toml`), except the letters and monograms, whose
outlines are glyphs of the bundled Fira Sans faces and are recorded under the
SIL Open Font License 1.1 (see `../fonts/README.md` and `../fonts/OFL.txt`).

The rest are converted from outside art that is free to ship, in
`imported/` (see "Imported items" below).

`MANIFEST.json` lists every item: its file under the library folder, kind,
intended size, cell size (reliefs), whether a vector is meant to be cut as a
closed shape (`closed`), search tags, the generator source file (under
`sources`) and license; the author is recorded once at the top. The `clipart::library` tests fail when the manifest and the
catalogue disagree; run

    CARVE_UPDATE_CLIPART_MANIFEST=1 cargo test -p carve-core --lib clipart::library

to rewrite it after changing the library, and review the diff.

Nothing is stored in the app bundle: the catalogue is listed from code, and an
item's file (`.carveclip` for reliefs, SVG for vectors) is written into
`Zeroed/clipart-library-<version>` under the platform cache folder the
first time it is drawn or inserted. Changing an item means raising
`LIBRARY_VERSION` so the cache is written afresh.

## Quality rules the tests enforce

- Reliefs use 0.1 mm cells (fine enough that a 1.5 mm ball nose finishing
  pass at 8 to 10 percent stepover shows no steps at the clip's own size),
  and are softened with a light blur so no one cell spike, pit or crease is
  left. The test fails on any one cell spike or pit over 0.15 mm, and when
  more than 3 percent of neighbor steps are cliffs over 0.5 mm.
- Vectors meant to be cut as shapes are closed loops that pass the Vector
  Validator (no open ends, zero length segments, duplicate nodes, self
  intersections or loops lying on each other), no two loops cross (they are
  apart or nested), and curves are fitted as lines and arcs within 0.02 mm
  so a loop stays under 400 segments. Molding profiles are open lines on
  purpose and marked `"closed": false`.
- Every item builds, writes, reads back, draws a thumbnail and inserts into a
  job.

## Imported items

`imported/` holds converted art from outside sources, never the originals:

| Set | Source | License | What |
| --- | --- | --- | --- |
| `polyhaven/` | Poly Haven 3D models (polyhaven.com/license) | CC0 1.0 | Reliefs rendered from sculptures, heads and a carved frame |
| `smithsonian/` | Smithsonian Open Access 3D scans | CC0 1.0 (each item's record read through the Open Access API) | Reliefs from carved panels, friezes, statues, fossils, sea life and a censer |
| `polyhaven-textures/` | Poly Haven textures | CC0 1.0 | Tileable texture reliefs from their displacement maps |
| `ambientcg/` | ambientCG materials (docs.ambientcg.com/license) | CC0 1.0 | Tileable texture reliefs from their displacement maps |
| `maki/` | Mapbox Maki icons (github.com/mapbox/maki) | CC0 1.0 | Map symbols as closed vectors |
| `phylopic/` | PhyloPic silhouettes (phylopic.org) | CC0 1.0 or the Public Domain Mark, per image | Animal silhouettes as closed vectors |

Every item's page, author, license and download day are in `MANIFEST.json`
(`origin`, `author`, `license`, `downloaded`); the test fails on an imported
item without them or with a license other than CC0 1.0, the Public Domain
Mark, CC BY 3.0 or CC BY 4.0. None of the shipped items is CC BY, so no
credit has to be shown; adding a CC BY item needs its author shown in the
app's credits as well.

Reliefs and tiles are stored as 16 bit grayscale PNG height maps (0 is no
material, a value `v` above 0 is `(v - 1) * 0.002` mm) at 0.2 mm (reliefs)
or about 0.1 to 0.2 mm (tiles) per pixel, and resampled to the library's
0.1 mm cells with a smooth cubic filter when an item is built. Vectors are
the source SVG with metadata stripped; building one welds its shapes by the
file's fill rule, drops specks under 0.4 square mm and fits each loop with
lines and arcs, thinning its points until it has at most 380 segments and
passes the Vector Validator.

The conversion is reproducible with `tools/clipart-import/`:

- `items.json` lists every imported item: the downloaded file (relative to
  a downloads folder), its conversion settings, name, category, tags and
  origin.
- `meshdepth/` is a small Rust program that renders a glTF, GLB or OBJ mesh
  to an orthographic depth map, smoothing flat facets with Phong
  tessellation (`cargo build --release` in that folder).
- `clipimport.py` (Python 3 with numpy, scipy and Pillow) turns depth maps
  into bas-reliefs (gradient compression and a Poisson solve, then a light
  median filter), displacement maps into tiles, and SVG into the stored
  vectors, and rewrites `src/clipart/library/imported/items.rs`:

      python tools/clipart-import/clipimport.py tools/clipart-import/items.json <downloads> .

  The downloads folder is not part of the repository; each item's `url`
  names the page it was downloaded from (Poly Haven through
  api.polyhaven.com, Smithsonian through 3d-api.si.edu, PhyloPic through
  api.phylopic.org, ambientCG through its download links, Maki from its
  GitHub repository).

Imported files add about 20 MB to the repository and to the app, which
embeds them.

## Outside sources checked

These were researched as possible additions. Each license was read on the
source's own page on 2026-09-26.

| Source | What | License as published | Notes |
| --- | --- | --- | --- |
| Openclipart (openclipart.org/share) | 2D vector clipart | CC0 1.0, "unlimited commercial use" | Uploads are by the public; some are traced from copyrighted art, so each item would need a provenance check. |
| Public Domain Vectors (publicdomainvectors.org) | 2D vector clipart | Stated as CC0 1.0 | Same provenance caveat as Openclipart; the terms page refused automated reading, so it is recorded here only from the site's own public domain page. |
| ambientCG (docs.ambientcg.com/license) | Textures with displacement (height) maps | CC0 1.0, may be included raw in commercial products | Good source for texture height maps; large files. |
| Poly Haven (polyhaven.com/license) | Textures, HDRIs, 3D models | CC0 1.0, may be redistributed in a product | Same use as ambientCG. |
| Smithsonian Open Access | 3D scans | CC0 for items marked CC0 only | Only items marked CC0; trademark and other rights still apply. |
| Mapbox Maki (github.com/mapbox/maki) | Map icons (animals, sports, transport) | CC0 1.0 | Small single color silhouettes. |
| game-icons.net | Silhouette icons | CC BY 3.0, attribution per author required | Allowed with attribution; would need a credits list. |


