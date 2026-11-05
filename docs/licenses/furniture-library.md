# Furniture library: licenses and sources

Every built-in furniture template lives in `crates/carve-core/src/furniture/library/<id>.json`
(format 1, see `docs/briefs/furniture-design.md`). A template either is our own design, released
under CC0-1.0, or follows an outside design whose license lets anyone use and change it in a
commercial app: CC0, public domain, CC BY or CC BY-SA. Designs under NC (non commercial) or ND (no
derivatives) licenses are never used. Each template file carries its own `license`, `author`,
`source` and `credit` fields; this page lists them all in one place.

Run `python3 tools/furniture-lint.py` after changing any template. It checks the format, evaluates
every expression at default, minimum and maximum values, and checks panel thickness, sizes and
overlaps.

## Templates

| Template | Id | Category | License | Author | Source | Credit line |
| --- | --- | --- | --- | --- | --- | --- |
| Bookcase | bookcase | see file | see file | written by the kernel work | see file | see file |
| Base Cabinet | base-cabinet | see file | see file | written by the kernel work | see file | see file |
| Wall Cabinet | wall-cabinet | Kitchen | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Drawer Unit | drawer-unit | Storage | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Nightstand | nightstand | Beds | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Desk | desk | Desks | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Coffee Table | coffee-table | Tables | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Side Table | side-table | Tables | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part I (1909), Project Gutenberg #23770](https://www.gutenberg.org/ebooks/23770), "How to Make a Tabouret" | After "How to Make a Tabouret", Mission Furniture: How to Make It, Part I, H. H. Windsor, 1909 (public domain) |
| Trestle Dining Table | trestle-dining-table | Tables | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Bench | bench | Seating | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part I (1909), Project Gutenberg #23770](https://www.gutenberg.org/ebooks/23770), the piano bench | After the piano bench in Mission Furniture: How to Make It, Part I, H. H. Windsor, 1909 (public domain) |
| Step Stool | step-stool | Kids | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Shoe Rack | shoe-rack | Storage | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Wine Rack | wine-rack | Kitchen | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Plant Stand | plant-stand | Decor | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part I (1909), Project Gutenberg #23770](https://www.gutenberg.org/ebooks/23770), "A Mission Plant Stand" | After "A Mission Plant Stand", Mission Furniture: How to Make It, Part I, H. H. Windsor, 1909 (public domain) |
| Kid's Chair | kids-chair | Kids | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Wall Shelf | wall-shelf | Decor | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Flat Pack Chair | flat-pack-chair | Seating | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Workbench | workbench | Shop | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Record Cabinet | record-cabinet | Storage | CC0-1.0 | CarveStudio contributors | our own design | none needed |
| Library Table | library-table | Tables | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "A Library Table" | After "A Library Table", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Umbrella Stand | umbrella-stand | Storage | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "An Umbrella Stand" | After "An Umbrella Stand", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Writing Desk | writing-desk | Desks | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "A Writing Desk" | After "A Writing Desk", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Magazine Rack | magazine-rack | Storage | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "Magazine Rack" | After "Magazine Rack", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Hall Tree | hall-tree | Storage | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "A Hall Tree" | After "A Hall Tree", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Settee | settee | Seating | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "Quarter-Sawed Oak Settee" | After "Quarter-Sawed Oak Settee", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Bookrack | bookrack | Decor | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "A Mission Bookrack" | After "A Mission Bookrack", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Porch Swing | porch-swing | Outdoor | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part III (1912), Project Gutenberg #23666](https://www.gutenberg.org/ebooks/23666), "A Porch Swing" | After "A Porch Swing", Mission Furniture: How to Make It, Part III, H. H. Windsor, 1912 (public domain) |
| Magazine Table | magazine-table | Tables | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part II (1910), Project Gutenberg #23991](https://www.gutenberg.org/ebooks/23991), "A Magazine Table" | After "A Magazine Table", Mission Furniture: How to Make It, Part II, H. H. Windsor, 1910 (public domain) |
| Smoking Stand | smoking-stand | Tables | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part II (1910), Project Gutenberg #23991](https://www.gutenberg.org/ebooks/23991), "A Smoking Stand" | After "A Smoking Stand", Mission Furniture: How to Make It, Part II, H. H. Windsor, 1910 (public domain) |
| Footstool | footstool | Seating | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part II (1910), Project Gutenberg #23991](https://www.gutenberg.org/ebooks/23991), "An Oak Footstool" | After "An Oak Footstool", Mission Furniture: How to Make It, Part II, H. H. Windsor, 1910 (public domain) |
| Cedar Chest | cedar-chest | Storage | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part II (1910), Project Gutenberg #23991](https://www.gutenberg.org/ebooks/23991), "An Oak-Bound Cedar Chest" | After "An Oak-Bound Cedar Chest", Mission Furniture: How to Make It, Part II, H. H. Windsor, 1910 (public domain) |
| Medicine Cabinet | medicine-cabinet | Storage | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part II (1910), Project Gutenberg #23991](https://www.gutenberg.org/ebooks/23991), "A Medicine Cabinet" | After "A Medicine Cabinet", Mission Furniture: How to Make It, Part II, H. H. Windsor, 1910 (public domain) |
| Telephone Stand | telephone-stand | Tables | Public domain | H. H. Windsor (Popular Mechanics Co.), adapted by CarveStudio contributors | [Mission Furniture: How to Make It, Part II (1910), Project Gutenberg #23991](https://www.gutenberg.org/ebooks/23991), "A Telephone Stand and Stool" | After "A Telephone Stand and Stool", Mission Furniture: How to Make It, Part II, H. H. Windsor, 1910 (public domain) |

Bookcase and base cabinet are written alongside the kernel; their license fields are in their files.

### About the public domain sources
Mission Furniture: How to Make It, Part I was published in 1909 by the Popular Mechanics Company,
Chicago (copyright H. H. Windsor). Works published in the United States before 1929 are in the
public domain, and Project Gutenberg distributes the book as such. Public domain needs no credit, but
the gallery shows the credit line anyway so makers can find the original plan. The three templates
keep the original's form and proportions (slab ends, keyed or wedged tenon stretcher, shelves
between arched ends, a top near 17 inches square on slab legs) and translate the solid oak
construction to flat panels and CNC joints; sizes are parametric, so the defaults are the 1909 sizes
rounded to metric where they suit modern use.

The second batch (14 templates) follows Mission Furniture: How to Make It, Part II (Popular
Mechanics Co., copyright 1910, Project Gutenberg #23991) and Part III (copyright 1912, Project
Gutenberg #23666), both public domain for the same reason. Each default size comes from the
plan's stock bill converted to millimeters (library table top 30 by 42 in, hall tree 59 in post,
settee 87 in rails and 36-1/2 in posts, porch swing 65 in seat, cedar chest 34-1/2 by 16-1/2 in,
and so on). Where a plan uses square posts and rails, the template turns each post and rail frame
into one slab panel with cutouts that keep the slatted look; where it uses solid stock wider than
sheet goods allow, the part is a panel of the same size. Changes a maker should know about:
- Library table: the 1912 drawers are left out (the aprons are solid); the end shelves sit between
  leg panels instead of square posts.
- Writing desk: the two drawers and the glass doors on the top cabinet are left out.
- Hall tree: the post, foot posts, braces and arms are two shaped boards that cross lap.
- Settee and porch swing: the slat frames are single panels with slots; the seat frame is a solid
  deck for a cushion.
- Smoking stand: the four curved legs are two cross lapped boards; the pipe rack is left out.
- Telephone stand: the matching stool is left out (it would be a separate piece).
- Cedar chest: the oak binding strips are left out; the base is a plinth under the box rather than
  a skirt around it, so the chest stands a little taller for the same box height.

### Designs looked at and not used
- OpenDesk and AtFAB furniture: mostly CC BY-NC (non commercial), so not usable here.
- Any design without a clear license that allows commercial use and changes is left out.
- USDA Forest Products Laboratory publications and Farmers' Bulletins are public domain and are a
  good place to look for future templates (sawhorses, farm shop benches); none are used yet.
- Handwork in Wood (Noyes, 1910, Project Gutenberg #20846) and Boys' Make-at-Home Things (1912,
  Project Gutenberg #60621) were read: the first teaches tools and joints rather than furniture
  plans, the second's workbench duplicates our workbench, so neither is used.
- From Part II and Part III, pieces built mainly from turned, bent or angled parts (rocking chair,
  curved back arm chair, book trough with a V trough, dictionary stand with a sloped top) were left
  for later.

## Sharing your own templates
A design you make in the Furniture Designer can be saved with Save as Template. It is written to the
app data folder "Furniture Library" as `<id>.carvefurniture.json`, in the same format as the built-in
files. To share it, send that file; the receiver uses Import Template... in the Furniture Library
dialog (or drops the file into their own "Furniture Library" folder, which Open Library Folder shows).

When you share a template:
- Set `license` to an SPDX id (CC0-1.0 if you want anyone to use it freely), `author` to your name.
- If it follows someone else's design, set `source` to that design's URL and `credit` to the line its
  license asks for, and only share it when that license allows changes and commercial use.
- Run `python3 tools/furniture-lint.py path/to/<id>.carvefurniture.json` first to catch mistakes.

## Report
Agent furn-lib, 2026-10-03, branch agent/furn-lib.

- Templates written (17): wall-cabinet, drawer-unit, nightstand, desk, coffee-table, side-table,
  trestle-dining-table, bench, step-stool, shoe-rack, wine-rack, plant-stand, kids-chair,
  wall-shelf, flat-pack-chair, workbench, record-cabinet. bookcase and base-cabinet left to furn-core.
- Designs followed: three from Mission Furniture: How to Make It, Part I (H. H. Windsor, 1909,
  public domain): tabouret (side-table), piano bench (bench), plant stand (plant-stand). The other
  14 are our own CC0 designs. NC and ND sources (OpenDesk, AtFAB) skipped.
- Lint: tools/furniture-lint.py (standard library only) passes 17 of 17. Each template is checked
  at defaults, all minimums, all maximums, each parameter alone at its min and max, every choice
  option and both states of every toggle. The lint caught a rail overlap in wall-shelf at minimum
  height; fixed by raising the minimum to 300 mm.
- Unsure: feature field names past `kind`, `edge` and `when` (u, v, w, h, r, rise, inset,
  diameter, at) are not fixed in the spec, so the kernel may need to map them. `{i+1}` in panel
  names follows the spec example. The lint does not classify connection types, so I checked by hand
  that each class's default joint is valid for its type (T rails kept more than one thickness from
  panel ends). The flat pack chair seat cross laps along the depth of the sides, a long slot; the
  kernel's crossLap must handle a horizontal part crossing a vertical one. Hardware row counts are
  expressions, like joint params.
- Pushed head SHA: given in the agent's reply (this file is part of that commit).
