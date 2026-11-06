# Kernel API contract (frontend <-> carve-core)

Transport: Tauri command `kernel_call(name: string, args: string)` where args
is JSON. Reply bytes: first 4 bytes ASCII tag `JSON` or `BLOB`.
- `JSON`: UTF-8 `{ "ok": true, "result": ... }` or `{ "ok": false, "error":
  { "code": "...", "message": "..." } }`.
- `BLOB`: `u32 header_len`, header JSON, then payload. Header:
  `{ "sections": [{ "name": "xy", "dtype": "f32"|"u32"|"u16"|"u8", "offset": n,
  "count": n }], ...extra fields }`. Offsets are bytes from payload start;
  each section starts on a four byte boundary (kernel `blob::frame`).
- Height sections (model.composite, sim.heightfield) are `u16` steps when
  that rounds no height by more than a hundredth of a cell: code `q` is
  `heightBase + q * heightStep` mm (both in the header), 65535 is NaN; else
  they are `f32` and the header has no `heightBase`. `bridge/blob.ts`
  `heightsOf` turns either into a Float32Array.
Frontend `bridge/client.ts` exposes `call<T>(name, args): Promise<T>` and
`callBlob(name, args): Promise<{header, sections: Record<string, TypedArray>}>`.
Errors reject with `KernelError {code, message}`.

Long commands: `kernel_call` runs off the shell's main thread, one command
at a time. Beside it, `kernel_progress()` answers `{running, progress}`
(0 to 1, 0 while the command does not report) and `kernel_cancel(id)` asks
the running command to stop (the devhost: `POST /kernel/progress` and
`POST /kernel/cancel`). A stopped command fails with code `cancelled` and
changes nothing. Nest Parts and 3D Finishing report progress and stop
(`carve_core::progress`); others run to the end.

Every mutating command returns `{ "revision": n }` (document revision, u64,
monotonic) merged into its result. The frontend refreshes what it mirrors
when revision changes. Ids are u64 as JSON numbers under 2^53.

## JSON shapes (serde `rename_all = "camelCase"`, enums tagged `kind`)
- `Point {x, y}` mm. `Segment`: `{kind:"line", to}` | `{kind:"arc", to,
  bulge}` | `{kind:"cubic", c1, c2, to}`. `SubPath {start, segments[],
  closed}`. `Path {subpaths[]}`. `Matrix [a, b, c, d, e, f]` maps
  `(x, y) -> (a x + c y + e, b x + d y + f)`.
- Entity JSON: `{id, kind, layer, sheet, name?, revision, ...}` with kind
  `path` -> `path: Path`; `text` -> `spec: TextSpec, origin: Point, paths:
  Path[]` (read only); `bitmap` -> `imageId, quad: Point[4], opacity`;
  `dimension` -> `spec: DimensionSpec`; `group` -> `children: number[]`.
  On `entities.create` the client sends the same object without `id` and
  `revision`.
- Ids are numbers. Colours are `#rrggbb` strings. Argument fields and
  command name segments are camelCase (`deleteOriginal`, `sinceRevision`,
  `layers.setActive`, `entities.hitTest`, `ops.node.setStart`) even where
  the tables below write them with underscores for brevity.

## Document
| name | args | result |
|---|---|---|
| ping | {} | {"pong": true, "version"} |
| doc.new | {} | revision |
| doc.open | {path} | revision |
| doc.save | {path} | revision |
| doc.restore | {path, as?} | revision; opens the autosave backup at `path` as the job `as` (none for a job never saved) with unsaved changes, so Save writes the job, not the backup; offered after a crash |
| doc.saveCopy | {path?} | {path}; writes a copy (default: `<name>.backup.carve` beside the job, or Untitled.backup.carve in the config Backups folder) without changing the document's path or dirty state; used by autosave |
| doc.variables.get | {} | {variables: {name: number}} |
| doc.variables.set | {variables} | revision; replaces every document variable (names: a letter then letters, digits, underscores; finite values), one undo step "Document Variables", saved in the .carve file |
| doc.variables.export / doc.variables.import | {path} | {count}; export writes one `name = value` a line; import reads such lines (also `name,value`, `#` comments), replacing variables of the same name, one undo step "Import Document Variables" |
| doc.info | {} | {revision, path, dirty, job, layers, sheets, active_layer, active_sheet, counts:{entities, components, toolpaths}} |
| job.get / job.set | {} / {job} | JobSetup |
| job.setScaled | {job} | {job, scaleX, scaleY, revision}; Job Setup with "Scale design with job size": sets the job and maps the old material rectangle onto the new one (p' = newMin + (p - oldMin) * scale per axis, a scale about the XY datum when there is no datum offset), moving every vector and relief component footprint of the active side; heights unchanged; one undo step "Job Setup" |
| job.setFlipped | {job} | {job, revision}; rotary Job Setup with Flip Design when the cylinder axis changes: sets the job and turns every vector and relief component of the active side a quarter turn counter clockwise about the old material centre onto the new centre; one undo step "Job Setup" |
| job.setSide | {side: "top"\|"bottom"} | {side, revision}; a view change: no undo step and the job is not marked changed (Undo and Redo turn to the side of the step they take back); bottom needs a double sided job |
| job.swapSides | {} | {side}; exchanges all content and toolpath sides between top and bottom |
| cam.importToolpaths | {path} a G-code file read into one toolpath per tool (kind imported); one undo step | {revision, ids, warnings} |
| cam.imported.get | {id} | {id, name, file, offsetX, offsetY, startDepth, feed, plunge, spindle, tool} |
| cam.imported.edit | {id, name?, offsetX?, offsetY?, startDepth?, feed?, plunge?, spindle?, tool?} moves (to the offsets given), deepens, renames or refeeds an imported toolpath; one undo step | {revision, id} |
| job.otherSidePaths | {} | {revision, paths}: the other side's paths and text outlines (active sheet, visible layers) turned over into this side's job space; empty for a single sided job |
| undo / redo | {} | {revision, label} |
| undo.info | {} | {can_undo, can_redo, undo_label, redo_label} |
| undo.beginGroup / undo.endGroup | {label?} / {} | {} / {merged}; the steps committed between them undo as one, called label or after the first; undo and redo close an open group; refused to gadgets |
| prefs.get | {} | Preferences plus {path, setAside} (see prefs.rs) |
| prefs.set | partial Preferences, groups merge field by field; unknown keys refused | as prefs.get; saved to prefs.json, undo limit applied |
| prefs.reset | {} | as prefs.get, every value back to its default |

## Layers and sheets
| layers.list | {} | [{id, name, colour "#rrggbb", visible, locked, active, count}] |
| layers.add | {name?, colour?} | {id} |
| layers.update | {id, name?, colour?, visible?, locked?} | keepOut (bool) marks the layer as keep-out zones: cam.compute warns when a toolpath enters its closed vectors. |
| layers.delete | {id, move_to?} | |
| layers.duplicate | {id} (copies the layer and every entity on it, named "<name> copy") | {id} |
| layers.set_active | {id} | |
| layers.reorder | {ids} | |
| sheets.list / sheets.add / sheets.delete / sheets.set_active / sheets.rename | analogous | |

## Entities
Entity kinds: `path`, `text`, `bitmap`, `dimension`, `group`.
`TextSpec.sizeMm` is the height of the capital letters (the Text
Height); the kernel derives the em size from each font.
| entities.list | {sheet?} | [{id, kind, layer, bbox:[minx,miny,maxx,maxy], closed, revision, name?, children?}] |
| entities.display | {ids?, since_revision?, tolerance?} | BLOB header {entities:[{id, revision, layer, kind, closed, selected, runs:[{offset, count}]}]}, section xy f32 pairs (count = points). Bitmaps return their placement quad in `xy` and an `image_id`. |
| entities.get | {id} | full entity JSON (Path segments, TextSpec, etc.) |
| entities.summary | {id} (any entity, group members too) | the summary entities.list gives: {id, kind, layer, bbox, closed, revision, name?, children?} |
| entities.create | {entity, layer?} | {id} |
| entities.replace | {id, entity} | |
| entities.delete | {ids, components?} (the relief components selected with them go in the same undo step "Delete"; components alone need no ids; an unknown component refuses the whole delete) | |
| entities.transform | {ids, matrix:[a,b,c,d,e,f]} (any move of vectors, this or another command: a shape component whose closed vectors all moved stays where it lies, its placement taking the move back, so a rebuild from them does not shift it) | |
| entities.set_layer | {ids, layer} | |
| entities.group / entities.ungroup | {ids} / {ids} | {id} / {ids} |
| clipboard.copy | {ids (top level entities), components?} (keeps copies of them and everything under them outside the document; replaces what it held) | {count} |
| clipboard.paste | {dx?, dy?} (new copies moved by the offset: entities on the active layer and sheet, groups as groups, text as text; components keep their settings, a shape pasted with its vectors is made from the pasted ones, one pasted without them stays linked to the originals only in the same job while they exist (after doc.new, doc.open or a template, or once they are deleted, it pastes as a plain component); the pasted entities are selected; one undo step "Paste") | {ids, components} |
| clipboard.state | {} | {count} |
| entities.probe | {x, y, radius, snap:{grid, guides, nodes, midpoints, centres, intersections, tangents, perpendicular}, exclude?} | {hits:[{id, kind:"node"|"mid"|"centre"|"on"|"quadrant"|"intersection"|"guide"|"grid", x, y, dist}], nearest_entity?} |
| entities.hit_test | {x, y, tolerance} / {rect:[..], mode:"inside"|"touch"} | {ids} |
| bitmaps.load | {path} | {image_id, width, height} and `bitmaps.pixels {image_id}` BLOB rgba u8 |
| bitmap.editPicture | {id? (first selected bitmap), contrast (-100..100), brightness (-100..100), gamma (0.1..10), invert, greyscale, border:"none"\|"rectangle"\|"oval", fade (0..50 % of the smaller side)} (writes a new PNG and points the bitmap at it; one undo step "Edit Picture") | {id, imageId} |
| bitmap.crop | {id? (first selected bitmap), boundaries? (the other selected entities)} (clears pixels outside the closed boundaries, even-odd, trims the image and its quad; one undo step "Crop Bitmap") | {id, imageId} |

## Selection
| selection.get | {} | {ids, bbox} |
| selection.set / add / remove / toggle / clear / all | {ids} | {ids} |
| selection.inside | {ids? (the selection when absent)} (smart part selection: keeps the closed boundaries selected and adds every pickable entity whose vectors all lie inside one of them; no undo step) | {ids, added} |

## Vector operations (all act on `ids` or the selection when omitted)
| ops.join | {ids, tolerance, smooth?} (smooth bridges gaps with a tangent curve) | {ids} |
| ops.close | {ids, mode ("smoothShallow": the symmetrical, shallower smooth method):"straight"|"smooth"|"moveEnd"} (two open vectors: their closest ends are joined; otherwise each is closed) | {ids} |
| ops.offset | {ids, distance, side:"outward"|"inward", corner:"round"|"sharp", delete_original} | {ids} |
| ops.weld / ops.subtract / ops.intersect / ops.xor / ops.crop | {ids} (subtract, intersect: the last id cuts or clips each earlier one) | {ids} |
| ops.extendTo | {ends:[{id, subpath, atEnd}], x, y} (each open end carried straight on to the point) | |
| ops.trimObjects | {ids (boundary last, kept), componentIds, side:"inside"|"outside", refit} | {ids, componentIds} |
| ops.fillet | {id, x, y, radius, kind:"normal"|"dogbone"|"tbone"} (a click on a fillet arc between two straight walls takes it out; else the nearest corner is filleted; a dog-bone circle runs through the corner; a t-bone runs along the wall the click is clearly nearer, else the longer) | |
| ops.filletProbe | {id, x, y, radius, kind} (changes nothing) | {action: "fillet"|"remove"|"none"} |
| ops.trimProbe | {id, x, y} (tries ops.trim in a step it throws away) | {trims} |
| ops.trim | {id, x, y} (interactive trim at a click point) | |
| ops.extend | {id, x, y} | |
| ops.move | {ids, dx, dy} or {ids, x, y, anchor} | |
| ops.set_size | {ids, width, height, link_xy, anchor} | |
| ops.rotate | {ids, angle_deg, cx, cy} | |
| ops.mirror | {ids, axis:"h"|"v"|"diag1"|"diag2"|"line", line?, copy} | {ids} |
| ops.align | {ids, mode:"left"|"right"|"top"|"bottom"|"hCentre"|"vCentre"|"centre"|"outsideLeft"|"outsideRight"|"outsideTop"|"outsideBottom", target:"selection"|"material"|"last"} (with `last`, the last selected id is the anchor and stays put; the outside modes put the others against the outside of the edge) | |
| ops.distribute | {ids, mode:"hLeft"|"hCentre"|"hRight"|"hGap"|"vTop"|"vCentre"|"vBottom"|"vGap", spacing: number or null} | |
| ops.arrayCopy | {ids?, components?, rows, cols, gapX, gapY, mode:"gap"|"offset", stagger, staggerFraction, group, symmetry?: four of "none"|"mirrorX"|"mirrorY"|"rotate" (the two by two block, first row first, about each copy's middle), rowShift?, colShift? ([x, y] mm added to every other row or column)} (components are copied into each cell; a shape copied with all its vectors is made from that cell's copies) | {ids, components} |
| ops.circularArray | {ids?, components?, cx, cy, count, angle, rotateCopies, radius?, group} (components go round with the vectors, as ops.arrayCopy) | {ids, components} |
| ops.copyAlong | {ids?, components?, pathIds (or one pathId), distributeMode, count, spacing, forceEven? (copies on both ends of each vector), circleDiameter? (Copy Circles: circles instead of copies), newLayer? (copies on a new layer "Copies Along Vectors"), alignToCurve, startOffset, reverse, deleteOriginal} (components are copied along with the vectors, as ops.arrayCopy, and deleted with them by deleteOriginal) | {ids, components} |
| ops.nest | {ids?, boundaryLayer? (the first closed vector on it bounds the nest and is not nested), params:{toolDiameter, gap, border (Border Gap, on top of the gap at the edges), rotationStepDeg, allowMirror, sheetWidth, sheetHeight (0 = job size), startCorner, direction, maxSheets, nestInHoles}, quantities:{"<id>": n} (else the object's nest mark, else 1), fillers:[id] (plus objects marked filler), sheet? (start sheet, default active), addSheets? (default true; false keeps every part on that sheet), sheetPlan?:[{sheet, quantity?}] (Custom Sheet Selection; each sheet nests at its own size unless a boundary or sheet size is given)} (packs each part onto sheets from the active one, adding sheets as needed; nestInHoles lets parts go inside holes of placed parts; filler parts are copied into the space left after the others, never opening a sheet) | {ids, sheetCount, unplaced:[{partId, copy, reason:"noClosedOutline"|"tooLarge"|"outOfSheets"}]} |
| ops.nestMarks | {ids?, copies (1 to 1000), filler?} (Individual Part Properties Apply: the objects keep the copies and filler mark, saved with the job, one undo step; 1 copy and no filler removes it; entity summaries answer it as `nest`) | {ids} |
| ops.distort | {ids? (selection when absent; [] for components only), componentIds?, mode? (box, curve or twoCurves), curves? (the one or two open vectors of a curve mode), angle? (Use Rotated Bounds frame, degrees; bbox and envelope in that frame), envelope (box mode):{bottom:[{x,y}], top:[{x,y}], left?, right?}, bbox?:[minX,minY,maxX,maxY] (default the box round ids and components), tolerance? (mm)} (bottom and top run left to right, sides bottom to top and are straight when absent; edges are Catmull Rom curves through their nodes and points map by their normalised place in bbox onto the Coons patch; vectors are resampled and refitted as lines and cubics, text becomes vectors, components are resampled on their own grid with heights unchanged; one undo step "Distort"; the distortion is kept on the vectors, so bending the same objects again starts from their geometry before the first bend until it is baked or they are edited another way) | {ids, componentIds} |
| ops.distortion / ops.bakeDistortion | {ids?} | {distortion: {ids, mode, curves?, angle, bbox, bottom, top, left?, right?} or null} / {baked} (undo step "Bake Distortion") |
| ops.snip | {x0, y0, x1, y1} (drag snipping: every plain vector the line crosses loses the crossed span back to the nearest intersections with other vectors, or its own ends when nothing bounds it; spans are worked out against the drawing before the drag; vectors left empty are deleted; one undo step "Snip"; geometry error when the line crosses nothing) | {ids (trimmed), deleted} |
| vectors.texture | {ids? (closed vectors; the job rectangle when none), params:{angle (-90..90 deg), maxSpacing, spacingVariation (0..100), wavelength, amplitude, noise (0..100), seed?}, preview?, layerName?} (rows of lines clipped even-odd to the boundaries; preview changes nothing; otherwise one undo step "Vector Texture", new vectors selected, on the named layer when given) | {ids} or {paths} |
| ops.selectDuplicates | {ids? (every pickable entity when absent), tolerance? (mm, default 0.01)} (selects every entity whose geometry lies within the tolerance of one earlier in z-order, both ways, whatever its direction, start node or node count; text only matches text; no undo step) | {ids} |
| ops.freehand | {points:[{x,y}], smoothing? (averaging passes, 0 to 50), tolerance? (fit tolerance mm, default 0.1), closed?} (smooths the trail and fits cubics, a new path on the active layer, selected; one undo step "Freehand") | {id} |
| measure.at | {x, y, tolerance (pick radius mm)} (read only) | {hit: null or {id, point, distance, length, closed, width, height, area (0 when open), spans, path, segment:{kind, length, start, end, radius?, centre?, sweepDeg?}}} |
| ops.array_copy | {ids, components?, rows, cols, gap_x, gap_y, mode:"gap"|"offset", stagger?, direction} (the grid steps over the box of the entities and components; each cell's component copies keep their settings, a shape made from vectors copied in the same cell from those copies) | {ids, components} |
| ops.circular_array | {ids, cx, cy, count, angle, rotate_copies, radius?} | {ids} |
| ops.copy_along | {ids, path_id, count or spacing, align_to_curve, start_offset, distribute_mode} | {ids} |
| ops.validate | {ids (empty: the visible vectors of the active sheet on visible layers), vgroove? (V-Groove Mode: crossings involving text left out)} | {issues:[{id, kind (open, selfIntersecting, intersection between vectors, zeroLengthSegment, duplicateNode, overlappingDuplicate), x?, y?, message, marks?:[{x,y}] (both ends of an open vector, each crossing)}]} |
| ops.fixZeroLength | {ids} (takes out every span shorter than the tolerance; undo step "Fix Zero Length Spans") | {removed} |
| ops.crossings | {ids} | {crossings:[{ids, x, y}]} where the vectors cross themselves or each other (at most 100; the V-Groove, Pocket and Offset intersection checks) |
| ops.simplify | {ids, tolerance, keep_arcs} | |
| ops.fitCurves | {ids?, tolerance, kind: "arcs"|"beziers"|"lines", sharpAngle? (Keep Sharp Corners: corners turning more than this many degrees stay nodes), replace? (default true; false keeps the originals and adds the fitted copies on the active layer, selected)} | {ids} |
| ops.fit_curves | {ids, tolerance, kind:"arcs"|"beziers"|"lines"} | |
| ops.node.* | insert/delete/convert/smooth/split/set_start on a path node; join (id, subpath, atStart) joins an open end to the nearest other open end | |
| ops.boundary | {ids, offset, kind:"selection"|"each"} | {ids} |
| text.fonts | {} | {families} (hidden and nameless system faces left out) |
| text.preview | {kind:"plain"|"arc"|"onCurve", ...the matching create args} | {paths} (changes nothing) |
| text.create / text.update | {spec: TextSpec, x, y} / {id, spec?, x?, y?, merge? (the revision the previous live edit answered: the edit joins that Create Text or Edit Text step of this text when nothing changed since)} | {id} |
| text.on_curve / text.arc | {spec, pathId, side, offset, startOffset} / {spec, cx, cy, radius, startAngleDeg, sweepDeg, inside} | {id} |
| dims.create | {DimensionSpec, customText?, textAt?} (layer from dims.style useLayer and layerName) | {id} |

## Import and export
| io.import_vectors | {path (svg, dxf, dwg, eps, ai, pdf), options:{layer, scale, units_hint, page (PDF page from 1)}} | {ids, warnings}; SVG images come in as bitmap entities |
| io.vectorPages | {path} | {pages}: a PDF's (or PDF based AI's) page count, 1 for other files |
| io.export_vectors | {path, format:"svg"|"dxf"|"eps"|"pdf"|"ai", ids?} | |
| io.import_bitmap | {path} | {id} |
| io.saveImage | {path, data (PNG as base64, a data: URL is fine)} (written as PNG, JPEG, BMP or GIF by the path's extension; JPEG, BMP and GIF laid on white) | |
| io.saveText | {path, text} | {}; writes the text as given (the cut list CSV) |
| io.readText | {path} | {text}; a UTF-8 file of at most 1 MiB (saved form settings) |

## Toolpaths (brief 008)
Toolpaths live in the document (undo, `.carve` files). Z is job Z. The
job's `machine` block holds `safeZ` (Z1), `plungeGap` (Z2) and the home
position (`homeX`, `homeY`, `homeZ` above the surface).
| name | args | result |
|---|---|---|
| cam.compute | {strategy:"profile"|"pocket"|"drill"|"vgroove"|"quickEngrave"|"rough3d"|"finish3d"|"chamfer"|"fluting"|"prism"|"texture"|"inlay"|"moulding"|"laserCutFill"|"laserPicture", name?, tool: Tool, params, ids} | {id, clearanceId, warnings}; clearanceId is the "<name> Clearance" toolpath added before it when params.clearanceTool is set and fits, else null |
| cam.inlayPlugVectors | {ids, sheet?} | {ids}; Plug Destination Sheet: mirrored copies of the inlay vectors on the sheet (active when absent), in place on their own sheet, centered on another; one undo step |
| (toolpath sheets) | | Toolpaths carry a sheet id (the active sheet when calculated; 0 in older files shows everywhere). cam.list and cam.get report it, cam.moves without ids draws only the active sheet's, sheets.duplicate copies a sheet's toolpaths with ids remapped to the copied vectors, sheets.delete deletes them. |
| (cam.compute notes) | inlay: params.part must be "female" or "male" (one toolpath per part; send two requests). moulding: ids[0] is the rail, ids[1] the profile. keyhole: params {direction:"bottomToTop"|"topToBottom"|"leftToRight"|"rightToLeft", depth, length}; at the bbox centre of each vector the tool plunges to depth, feeds length along the direction and back, and lifts (kind "keyhole"). laserCutFill (laser tool): params {mode:"cut"|"fill"|"cutAndFill", side:"on"|"outside"|"inside" (half the kerf), passes, power?, speed?, fillPower?, lineInterval, angle, hatch:"lines"|"cross", bidirectional, overscan}. laserPicture (laser tool, ids may be empty): params {path, origin:{x,y}, width, height (0 keeps the aspect), lineInterval, mode:"powerModulation"|"dither", dither:"floydSteinberg"|"ordered", minPower, maxPower?, speed?, invert, bidirectional, overscan}. Any flat strategy takes params.projectToModel (bool): the moves are dropped onto the relief model (cuts split into 0.5 mm spans, arcs become lines) and the toolpath records the model fingerprint so it goes stale. Laser moves carry `power` (percent) per move; posts write JET_TOOL_POWER when it changes. | |
| (cam.compute placement) | selectedGroup? (the group picked in the list): with toolpaths.appendToSelectedGroup on, the new toolpaths go to the end of that group. Area strategies first repair closed vectors that cross themselves, for at most toolpaths.geometryFixSeconds (0 repairs none), with a warning for each repair and for a timeout. | |
| cam.duplicate | {id, name} (empty name: "<name> Copy") | {id, clearanceId, restIds, warnings}; recalculated from the original's request, placed straight after it in its group unless toolpaths.appendDuplicated (then the end of the list) |
| cam.depthCheck | {} | {all, toolpaths:[{id, name, depth, thickness}]}: toolpaths cutting further below their material's bottom than their own through cut allowance (the toolpath's `throughCut`, measured when it was calculated from its settings and kept by a recalculation from stored settings); hidden ones only when all (toolpaths.checkAllDepths) |
| cam.recalculate | {id, request?} (request as cam.compute; stored one when absent) | {id, warnings} |
| cam.list | {} | [{id, name, kind, visible, stale, toolName, toolNumber, estimatedSeconds, moves, gadget?}] (gadget: name of the gadget that made it, or null) |
| cam.get | {id} | {id, name, kind, visible, stale, tool, request, estimatedSeconds, bounds, gadget} (gadget: {name, values} of the gadget run that made it, or null) |
| cam.delete / cam.reorder | {ids} | |
| cam.rename | {id, name} | |
| cam.setVisible | {ids, visible} | |
| cam.moves | {ids?} (visible ones when absent) | BLOB header {revision, toolpaths:[{id, offset, count, visible}]}, sections xyz f32 triples, width f32 (the width the tool cuts at the material surface at each point, 0 above it) and kind u8 (0 rapid, 1 feed, 2 plunge, 3 arc step, 4 dwell, 5 retract: a lift straight up, 6 join: a flat feed from a closed contour to the next; a feed straight down is a plunge); each toolpath starts at home |
| cam.jobSheet | {path, title?} | {path}; writes a self contained HTML job sheet: layout picture, material setup, and per sheet and side a summary table and a detail block (tool, depths, feeds, spindle, time) for every toolpath. Fails when there are no toolpaths |
Params: profile {startDepth, cutDepth, passDepth?, side:"outside"|"inside"|"on",
direction:"climb"|"conventional", allowance, tabs:{enabled,count,length,thickness},
ramp:{enabled,distance}}; pocket {startDepth, cutDepth, passDepth?, stepover?,
strategy:"offset"|"raster", rasterAngle, direction, allowance, profilePass, ramp};
drill {startDepth, cutDepth, peckDepth, fullRetract, dwell}; vgroove
{startDepth, flatDepth?} (tool must be a V-bit or engraving tool);
quickEngrave {depth, passes, mode:"outline"|"fill", stepover, angle};
finish3d {stepover?, angle, allowance, boundary:"model"|"material"|"selection"};
rough3d {the same plus passDepth?}. 3D strategies need no ids unless the
boundary is "selection"; they machine the composite placed with its top at
the material top.

## Preview (brief 009, first cut)
| sim.run | {ids?, quality:"standard"|"high"|"veryHigh"|"extremelyHigh"|"maximum"} (visible toolpaths when no ids) | {width, height, cellSize, collisions:[{toolpathId, moveIndex, x, y, z, kind:"rapid"\|"plunge"\|"flute"\|"shank"\|"holder"\|"clamp", severity:"high"\|"medium"\|"low"}], seconds}; the same sweep and checks as the animation (cam/collide.rs): each kind once a move, moves of one kind within 6 of each other one event |
| sim.check | {tolerance?} (mm, default 0.1; needs a preview) | BLOB header {width, height, tolerance, found, issues:[{kind:"gouge"\|"leftover"\|"undercut", severity, toolpathId, moveIndex, x, y, z, depth, area}]} (worst first, at most 100; leftover under 2 sq mm only on the map) and section `check` u8 per cell (0 clean, 1 gouge, 2 leftover, 3 undercut); cam/deviation.rs |
| sim.reset | {} | |
| sim.texture | {path} (a material texture picture, at most 32 MB) | blob {mime} with the file bytes |
| cam.prismDepth | {tool, ids} | {depth} (mm below the start depth for full prisms) |
| cam.templateLayers | {path} | {missing:[layer names], count} |
| cam.loadTemplate | {path, missing?:"delete"\|"create"\|"asIs"} | {ids, skipped, uncalculated} |
| sim.heightfield | {} | BLOB header {width, height, cellSize, originX, originY, top, bottom, revision, heightBase?, heightStep?}, sections z heights (rows from lowest Y, NaN where cut through), owner u16 (toolpath list position + 1) and floor heights (the underside), left out while every underside is `bottom` |
| sim.advance | {distance?, oneMove?, toToolpathEnd?, toRetract?, toEnd?, toMove?:{toolpathId, moveIndex, x, y, z}} as sim.step (needs sim.start; toMove stops where that move passes nearest the point) | BLOB like sim.patch (the cells changed since the last patch: header `rects` [[x0, y0, width, height], ...], the changed 32 cell tiles joined into rectangles; z, owner and floor sections hold the rectangles one after another row by row; floor is left out when every one of those cells has its underside uncut) with the step's answer {done, tool, collisions, progress, seconds} as header `step`; one call per animation frame |

## Relief model (brief 011, first slice)
Components live in the document (undo; heights saved as
`heightfields/<id>.f32` in the .carve zip). Heights are mm above the model
zero plane; the grid covers the job at about a million cells.
| model.createShape | {ids, params:{profile:"round"|"angular"|"flat", angle, height, limit, baseHeight}, combine:"add"|"subtract"|"mergeHigh"|"mergeLow", name?} | {id} |
| model.importMesh | {path (.stl, .obj, .3ds, or .dxf with 3DFACE entities or polyface meshes), params:{up:"z"|"y"|"x", width?, centre?}, combine, name?} | {id, triangles} |
| model.import.info | {path, orient:{top:"top"|"bottom"|"front"|"back"|"left"|"right", rotX, rotY, rotZ (degrees), size?:[x,y,z] mm, unitScale (mm per file unit), perspective 0..1, centre?}} | {triangles, natural:[x,y,z] file units after turning, size:[x,y,z] mm, centre} |
| model.import.preview | {path, orient} | BLOB header {size, centre, count}; sections xyz f32 (9 per triangle, job mm, standing on Z 0), segment u16, flags u8 (bit 0 undercut); thinned above 400k triangles |
| model.import.apply | {path, orient, place:{zeroPlane mm above the model bottom, bothSides, discardBelow}, combine, name?, replace?:[ids]} | {ids, otherSide?: id of the underside component on a double sided job} |
| model.import.segmentPreview | {path, orient, plan:{root:{cut?:{point:[x,y,z], angleX, angleY}, children:[above, below]}, merges:[[leafId]], layouts:{segmentId:{quarterX, quarterY, quarterZ, rotX, rotY, rotZ}}, caps, resizeMaterial, boundaryVectors, gap}, layout: bool} | BLOB as preview, header adds segments [{id, leaves, size}] and extent; leaf ids are "r" then one digit per level |
| model.import.segmentApply | {path, orient, plan, combine, name?, replace?} | {ids, vectors} (one component per segment, laid out) |
| model.import.settings | {id} | {source: {path, orient, place?, plan?, session, combine} or null, ids: every component of that import} |
| model.sweep | {rails:[idA, idB], sections:[ids], params:{scaleHeightWithWidth, baseHeight}, combine, name?} | {id} |
| model.turn | {id (profile vector), params:{baseHeight}, combine, name?} | {id} |
| model.spin | {id, params:{baseHeight, startAngle, endAngle}, combine, name?} | {id} |
| model.edit | {id, op:"smooth"|"draft"|"tilt"|"fade"|"mirror"|"flip"|"scale"|"emboss", params} (fields as model/edits.rs, camelCase) | |
| model.sculpt | {id, points:[{x,y}], params:{brush:"smooth"|"deposit"|"remove"|"smudge"|"erase", diameter, strength 0..1, falloff 0..1, limit?, grow}, custom?: {brush (component id) or file (a .carveclip or picture path; kept until sculptEnd), angle, dynamic, merge}, pressures?: [0..1 per point, a graphics tablet pen's pressure scaling the strength]} (one undo step per stroke; custom works with deposit and remove) |
| model.createBrush | {id, name?, dir?} writes the component as a sculpting brush clip (cropped, at most 256 cells a side) into the brushes folder (or dir), never overwriting; no undo step | {path} |
| model.brushFolder | {} makes and answers the brushes folder | {path} |
| model.transform | {id, params:{offset:{x,y}, scaleX, scaleY, rotateDeg, centre?}, replace?} (identity by default; heights unchanged; `replace` = the revision a previous step of the same live drag answered, taken back first while it is the newest) | |
| model.replaceBelow | {ids?, level?, height, with: transparent\|plane, lower} (one component in place; several, or a level's visible ones, baked into one first; height as the chosen components show on their own) | {id, revision} |
| model.fromBitmap | {path or id (a placed bitmap entity: the component lies exactly under it, origin and size ignored), origin?:{x,y} (bottom left, default centred), pixelSize? or width? (mm; default fits the job), height (mm between black and white, default 5), baseHeight (default 0), invert (dark is high), smoothing (3 by 3 blur passes 0..20, default 1), combine, name? (default the file name, or the bitmap's name)} (grey levels on the model grid, transparent pixels empty, images over 4 Mpx averaged down; one undo step "Component from Bitmap") | {id} |
| model.boundary | {ids? (components; all visible when absent), offset, level? (trace height; the silhouette of everything they cover when absent), group? (several loops come back as one group)} | {ids} of new vectors (the group's id when grouped) |
| model.transformMatrix | {ids, matrix, scaleZ? (Auto Scale Z: height scale times the square root of the area scale) [a,b,c,d,e,f] as entities.transform, replace?} (the components go with a 2D selection's move, scale, rotate or mirror; one undo step "Transform Components"; the shape source records the move in its placement and rebuilds keep it; its vectors moved by the same matrix, before or after, leave it lying on them; `vectors` is still accepted and not needed) | |
| model.copyMatrix | {ids, matrix, vectors?} | {revision, ids, vectors}; copies of the components moved by the matrix (texture areas keep their tiles, shapes keep their Create Shape settings) and of the `vectors` (any kind, groups whole), which become the selection; a shape whose vectors were copied too is made from the copies, any other records where it lies from its original vectors; one undo step "Copy" ("Copy Components" without vectors) |
| model.crossSection | {from:{x,y}, to:{x,y}, scale? (height multiplier, default 1), origin?:{x,y} (default the job's top left plus 10 mm up)} (open polyline on the active layer: distance along X, height times scale along Y) | {id, length, maxHeight} |
| model.slicePreview | {thickness or thicknesses:[..] (last repeats), fromTop? (thicknesses taken down from the top, the part slab at the bottom), count? (that many equal slabs, in place of the thicknesses), heights? (the Z values to cut at, the Slice Heights list, in place of both), componentIds? (default the visible model)} | {maxHeight, slabs:[{index, bottom, top}]} |
| model.slice | same as slicePreview plus name? (default "Slice"), boundaryVectors? (each slab outlined with closed vectors on a new layer named after it) (sources are hidden; each slab becomes an Add component dropped to 0, named "Slice 1..n", which add back up to the model) | {ids, hidden, slabs, vectors, layers} |
| model.split | {componentId, vectorIds, mode?:"split"\|"clearInside"\|"clearOutside"} (closed vectors, or one open vector carried on in straight lines past the model's edges whose left side is the inside; split keeps the outside part in the component as "<name> - A" and adds the inside "<name> - B" just above it) | {id, insideId?} |
| model.addZeroPlane | {} (a zero height component the size of the job, on the active level) | {id} |
| model.smoothComponent | {id, amount 0..100, preserveTransparency? (true), replace?} | {id} |
| model.embossComponent | {id, scale (percent kept, above 0 to 100), detailSmoothing? 0..100, replace?} | {id} |
| model.scaleHeight | {percent or height (mm of the model top), replace?} (multiplies every component's height scale and base height) | {factor, maxHeight} |
| model.volume | {ids?} (the composite model, or the components ids baked together; heights below the zero plane count their depth) | {volume (mm cubed), empty, selected (components measured, 0 for the composite)} |
| model.offsetModel | {distance (mm, negative shrinks), clipToZero?, merge?:{revision, id}} (visible model to a new component, sources hidden; merge offsets that component again inside the same undo step) | {id, hidden?} |
| model.addDraft | {angleDeg} (visible model with a draft skirt as a new Merge High component "Draft N°") | {id} |
| model.fromPreview | {mode?:"whole"\|"overModel"\|"leftOver"} (needs sim.run first) | {id} |
| model.revertPreview | {revision} (undoes a smooth, emboss, scale height or offset step still at that revision) | {reverted, revision} |
| (replace) | `replace` is the revision a previous call of the same command answered; when the document is still at it, that step is undone first, so a live preview stays one undo step | |
| model.exportStl | {path, ids?, base (mm under the lowest point, default 1)} | |
| model.exportMesh | {path, format?:"stl"\|"obj"\|"pov" (default from the extension), ids?, base (gap to the flat or mirror plane, default 1), back?:"open"\|"inverted"\|"flat"\|"otherSide"\|"sides" (default flat; otherSide needs a double sided job with a model on the other side, sides a rotary job; both take the whole model), tolerance? (mm) or maxTriangles? (reduction; every cell when neither)} (OBJ also writes <name>.mtl and <name>.jpg of the component colours; POV is a scene with the model as a mesh2, Y up) | {format, triangles, deviation} |
| model.meshStats | the same arguments without path and format | {triangles, deviation} |
| model.meshPreview | as model.meshStats | BLOB header {triangles, deviation}, f32 section xyz: nine numbers a triangle, job mm (a rotary sides model round its axis) |
| model.exportHeightmap | {path, bits?:8|16 (default 16), ids?} (greyscale PNG, one pixel per model cell over the job, the visible composite or the chosen components; black is the lowest of the model and 0, white the top) | {width, height, minZ, maxZ, pixelSize} |
| model.list | {} | {stamp, components:[{id, name, visible, combine, heightScale, level, baseHeight, color ("#rrggbb" or null), group (0 for none), maxHeight, bbox:[minX,minY,maxX,maxY]}], levels:[{id, name, visible, combine, groups:[{id, name, visible, combine, expanded, color}], clip:[vector ids]}] bottom first, activeLevel} (stamp moves only when the model changes; combine is "add"\|"subtract"\|"mergeHigh"\|"mergeLow"\|"multiply") |
| model.update | {id, name?, visible?, combine?, heightScale?, level?} | |
| model.levels.add | {name?, combine?} (goes on top and becomes active) | {id} |
| model.levels.update | {id, name?, visible?, combine?, color? ("#rrggbb" covers every component in it, null takes theirs), material? (name of the preview material the colour came from, null for none)} | |
| model.levels.delete | {id} (components move to the level below; the last level cannot go) | |
| model.levels.reorder | {ids} (bottom first) | |
| model.levels.setActive | {id} (new components go there) | |
| model.delete / model.reorder | {ids} (reorder is bottom first) | |
| model.composite | {ids?, otherSide?} (only those components, as they show alone; the whole visible model when absent; otherSide: the other side's model turned over onto this side's grid, an error when it has none) | BLOB header {revision, width, height, cellSize, originX, originY, min, max (exact), heightBase?, heightStep?}, section z heights |
| model.props | {ids, name? (one id only), visible?, combine?, heightScale? (shape height, 1 = 100 %), baseHeight? (mm riser under the whole footprint, after the scale), color? ("#rrggbb", null clears), material? (preview material name the colour came from, null for none), replace? (revision of the previous step of a live drag, which this one replaces)} (one undo step for all ids) | |
| model.groups.create | {ids, name? (default "Group n"), combine? (default add)} (the group goes on the level of the topmost member, members sit together where that member was; a group bakes its members from zero, then meets the level with its combine mode) | {id} |
| model.groups.update | {id, name?, visible?, combine?, expanded?, color? ("#rrggbb" overrides the members, null takes theirs), material?} | |
| model.groups.ungroup | {id} (members stay where they are) | {ids} |
| model.groups.transform | {id, params (as model.transform; centre defaults to the middle of the whole group)} (every member, one undo step) | |
| model.tree.move | {ids, level, group? (0 or absent for none), before? or after? (component id; default the top of the target group or level)} (drag and drop in the tree; emptied groups are dropped) | |
| model.levels.setClip | {id, vectorIds} (Clipping Apply: the closed vectors' shape is kept on the level; empty removes the clipping) | |
| model.levels.updateClip | {id, vectorIds?} (Clipping Update: reads the given vectors, or the last ones again when none) | |
| model.fromVisible | {ids? (default the whole visible model), name?} (Create Component from Visible Model: a new Add component; the visible sources are hidden so the model looks the same) | {id, hidden} |
| model.colors | {} | BLOB header {revision, width, height, any, materials (the preview material of each palette entry, null for a plain colour; one colour from two materials gets two entries)}, sections palette u8 (RGBA per entry, entry 0 clear) and index (the entry of each composite cell: the topmost component's colour; u8 up to 256 entries, u16 beyond) |
| model.highlight | {ids} | BLOB header {revision, width, height}, section mask u8 per composite cell (1 the components show there, 2 others hide them, 0 elsewhere) |

## Clipart library (brief 013g)
Folders of clips; each subfolder is a category. Lists `.carveclip` (our own
single component relief file: zip of `clip.json` {format, kind:"relief",
name, width, height, cell} and `heights.f32`), STL/OBJ (relief) and
SVG/DXF/EPS/AI (vectors). The shipped library (`clipart/library`, all made
by our own code, listed in `crates/carve-core/assets/clipart/MANIFEST.json`)
is always listed first with categories like "Shapes/Domes"; an item's file is
written under `Zeroed/clipart-library-<version>` in the cache dir the
first time it is drawn or inserted. The user folder is `Zeroed/clipart`
under the platform data dir.
| clipart.scan | {dirs? (replace the user folder), library? (alias samples; default true)} | {categories (sorted, `/` between levels), items:[{name, path, category, kind:"relief"\|"vector", format:"clip"\|"mesh"\|"vectors", tags?}], userDir, roots, warnings} |
| clipart.thumbnail | {path, size? (16..1024, default 256)} | BLOB header {width, height, mime:"image/png"}, section png u8 (PNG file bytes, square, clear background) |
| clipart.insert | {path, x, y (where the clip's centre goes), scale? (default 1, heights scale too), combine?} | {kind:"relief", id} or {kind:"vector", ids} (vectors go on the active layer and are selected); one undo step "Insert Clipart" |
| clipart.save | {componentId, path, name?} (".carveclip" added when the path has no extension; height scale baked in) | {path} |

## Job templates (session 4)
A template is a `.carve` file. The user's templates live in
`Zeroed/templates` under the platform data dir (or the folder the
`CARVE_TEMPLATES_DIR` environment variable names); files at the top are
listed with category null, each subfolder is a category. Five standard sheet
templates (plywood 2440 x 1220 x 18 mm, MDF 1220 x 610 x 12 mm, hardwood
600 x 400 x 19 mm, plywood 48 x 96 x 0.75 in, sign blank 24 x 18 x 0.5 in)
are written on demand under the cache dir and listed as category "Standard".
Gadgets may not run `templates.*`.
| templates.list | {} | {userDir, templates:[{name, category (null at the top), path, standard, units, width, height, thickness (mm), layers, vectors, toolpaths}] (Standard first, then the user's by category and name, case insensitive), warnings:[{path, message}]} |
| templates.save | {name, category?, design? (default true: vectors and components), toolpaths? (default true, only with design), overwrite? (default false)} | {path}; job setup, layers, sheets, notes, variables and levels always; no selection or undo history; names may not contain / \\ : * ? " < > \| or start or end with "."; an existing template needs overwrite |
| templates.new | {path} | {revision, name}; the template becomes a new untitled document (no path, clean, empty undo history); a file that does not open leaves the document alone |
| samples.open | {id} | {revision}; a finished job built into the app (ids in `crates/carve-core/src/samples.rs`) becomes a new untitled document in the preferred units (no path, clean, empty undo history). |

## Gadgets (brief 013e)
Lua 5.4 scripts in `<user data>/Zeroed/gadgets/<name>/<name>.lua` (plus
optional `gadget.json` `{title, description}`), and three shipped ones
(`gear-maker`, `keyhole`, `rounding`). A script declares `gadget = {title, description,
dialog = {fields = [{id, label, kind: "number"|"length"|"choice"|"check"|"text"|"tool" (a tool database id, 0 for none; carve.tool(id) fetches it),
default, options?, min?, max?, tooltip?, group? (section heading; neighbours with one group share a section), enabledBy? (id of a check field;
unticked, the field is greyed and not sent, so the default applies)}]}, selection? (message: the form shows it and will not run while nothing is
selected)}` and `function run(values)`.
Lengths are mm. Inside `run`, `carve.call(command, args)` runs any kernel
command except file, undo, settings and gadget ones and answers its result;
`gadgets/prelude.lua` adds helpers (job, layers, contour builder, circle,
rectangle, arc, selection, tools, toolpath, message). Sandboxed: no io, os,
package, require, load, dofile, debug; memory cap; instruction budget.
| name | args | result |
|---|---|---|
| gadgets.list | {} | {gadgets:[{name, title, description, origin:"bundled"\|"user", error?}]} |
| gadgets.describe | {name} | {name, title, description, origin, needsSelection?, lastValues?, fields:[{id, label, kind, default, options?:[{value, label}], min?, max?, tooltip?, group?, enabledBy?}]}; lastValues: the values of the gadget's last successful run (prefs gadgets.lastValues) |
| gadgets.run | {name, values:{id: value}, toolpathId?} (missing values take the default) | {messages:[string], revision, toolpathIds}; one undo step labelled with the title; a Lua error rolls everything back and fails with its message. Toolpaths the run makes record the gadget and its values (cam.list `gadget`, cam.get `gadget:{name, values}`, saved in the file). With toolpathId (a toolpath this gadget made) the vectors its earlier run drew go first unless changed since, and the first toolpath the run makes replaces it in place (same id and position); toolpathIds then starts with it |

## Rotary jobs (session 4)
JobSetup.rotary is {diameter, axis:"x"|"y", length, blank:"cylinder"|"square", zZero:"centre"|"surface"}
(length, blank and zZero default to 300, cylinder, centre in older files). job.set with
kind "rotary" derives the flat layout from the blank: width x height is length x (pi *
diameter) along X, swapped along Y; thickness is the radius; zZero becomes "bottom" for
the axis or "top" for the surface. Toolpaths are computed flat.
| cam.save (rotary) | as cam.save; the kernel adds `rotary` from the job | a rotary job is wrapped: the post must declare `ROTARY_WRAP = A` (blank along X, flat Y becomes A degrees = y / (pi d) * 360, Y written as 0) or `ROTARY_WRAP = B` (along Y, X becomes B); arcs are written as straight moves; Z unchanged. Otherwise a Validation error names the post |
| rotary.unwrapVectors | {ids? (the selection when absent), direction?: "unwrap"\|"wrap"} (rotary jobs only; one undo step "Unwrap Vectors") | {ids}; each vector is flattened and mapped across the axis about the layout centre line: unwrap d -> r asin(d/r) (a side view drawing to the unwrapped surface), wrap s -> r sin(s/r). Results are polylines |
| rotary.unwrapContour | {id? (the one selected vector when absent), centre?:{x,y} (the middle of its box when absent), refine?} (a closed contour unrolled about the center: a new open vector pi times the blank diameter long, its height the outermost distance from the center every half degree from the right anticlockwise; refine simplifies and fits arcs; rotary jobs only; one undo step "Vector Unwrapper", the new vector selected) | {id} |
cam.save takes `rotaryWiring` {letter:"A"|"B"|null, units:"degrees"|"linear", reversed} from the machine: the turn goes out on that letter (a post with both tokens names it once), linear writes the distance round the blank in the post's units. Post tokens [A] and [B] are the angles (absolute or incremental like X). posts.list
summaries carry `rotary: bool`. Summaries also carry `rotaryWrapY` and `rotaryWrapX`: the letter a blank along X (flat Y) or along Y (flat X) is written on, or null. Post settings: `ROTARY_WRAP_Y = A` wraps flat Y onto A, `ROTARY_WRAP_X = A` or `B` wraps flat X, plain `ROTARY_WRAP = A` is the Y form and `= B` the X form; `MAXIMUM_ROTATION` (at most 179) breaks longer turns into pieces. Shipped rotary posts: grbl_rotary_a_mm (Y to A), mach3_rotary_mm (Y to A, X to B), and Y to A and X to A on linuxcnc_rotary_mm, linuxcnc_rotary_inch, linuxcnc_manual_rotary_mm, linuxcnc_manual_rotary_inch, uccnc_rotary_mm, uccnc_manual_rotary_mm.

## Multi tool toolpaths (session 6)
cam.compute for finish3d takes params.restTools (tools after the request's own, in cutting
order), minimumDetail and restBoundaryOffset (mm). The reply adds restIds: one toolpath per
later tool that has something left, named `<name> [<tool>]`, after the first. Their record
holds part "rest", restIndex, mainTool, baseName and family (the first toolpath's id, which
the first carries too). cam.get of any member answers the whole request. cam.recalculate
with a request on any member redoes the family: members replaced in place, new tools added
after it, dropped tools deleted (one undo step); without a request only that toolpath.

## Toolpath Tiling (session 6)
Tiling = {enabled, spec: {method: "individual"\|"feedX"\|"feedY", tileWidth, tileHeight,
overlap, smallestFirst}, active, originalPosition}; saved in document.json as `tiling`.
Each tile reaches back into the one before by the overlap; the tile's own corner is moved
to the job's minimum corner, so the overlap is cut at negative coordinates.
| name | args | result |
|---|---|---|
| cam.tiling.get | {} | Tiling plus tiles:[{row, column, nominal:[4], area:[4], shift:[dx, dy]}] |
| cam.tiling.set | {tiling} (one undo step "Toolpath Tiling"; active is clamped) | as get, plus revision; Validation for sizes that make no tiles, or for enabled tiling when the XY datum is not at the bottom left (as is cam.save with tiling) |
| cam.tiling.view | {active?, originalPosition?} (not an undo step) | as get, plus revision |
| cam.moves (tiling on) | as cam.moves | only the active tile's moves, at the origin, or in place when originalPosition |
| cam.save (tiled) | as cam.save plus outputTiled: true | one file per tile, `<stem>_tile_<row>_<column>`; Validation when tiling is off |
| posts.changelog | {id} | {id, entries:[{who, when, what}]}: the History table in the post's leading comments (`' History`, then rows split on two or more spaces; one column carries on the row above; empty when there is none) |
| cam.save | {toolpathIds (cutting order), post, path, output?: "selected"\|"oneFile"\|"multipleFiles" (default oneFile; older `oneFilePerToolpath` means multipleFiles), groupWherePossible?, outputAllVisible?, outputTiled?} | {paths, tiles?}. multipleFiles writes `<stem>_<toolpath name>` each, or with grouping neighbours with the same tool (number and cutter) in `<stem>_<first>-<last>` (list places, from 1). A file holding more than one tool needs a post with a TOOLCHANGE block (posts.list `toolChange`) and each cutter its own tool number, else Validation; one side per file; a post with TAPE_SPLITTING writes a long program as several files named by its format, all listed in paths |

## assembly.* pieces (the Assembly View)

One piece per part a sheet's toolpaths cut free (sheets that own a
toolpath; sheet 0 toolpaths count for every sheet): the sheet's visible
toolpaths are simulated on its own material and split into regions of
material that a through cut separates from the waste. Tabs count as cut
through (material a through cutting toolpath left thinner than half the
material), regions that reach the sheet edge are waste, a region lying in a
hole of another part is a slug (waste) unless a visible toolpath cut with
`params.side` "outside" (profile, laser) has a closed source vector whose
box fits the region's within max(3 cells, 2 mm) and whose area is within
25% (a part nested in another part's hole), or one cut with side "on" has
a closed source vector that, shrunk by the tool radius, fits the region so,
its kerf is more than half of the cells round the region (the "on" cut is
what parts it), and the region is at least two tool diameters across (a
narrower slug is a hole's waste); crumbs under 4 cells are dropped.
Parts are numbered from 1 in the order their first cell turns up, row by row
from the lowest Y. A sheet with nothing cut free is one piece, the whole
sheet. Raw model: a sheet with visible components that rise above their
base (the sheet's composite on its own model grid) and no toolpaths gives
its model as its one piece, part 0; so does a sheet with toolpaths listed
in the assembly's `modelSheets` (`assembly.pieceSource`). Component pieces:
each component in the assembly's `components` (`assembly.addComponentPieces`)
is a piece of its own, `{sheet (its sheet), part: 1, component}`, listed
after its sheet's pieces: the component baked alone on its sheet's model
grid (shown even when hidden, added whatever its combine mode, no level
clipping), standing on its lowest point like a model piece; it is left out
of its sheet's raw model. Its placement is stored with `component` and no
anchor. The model piece's
material stands from the model's lowest point (zero or below) up to its
heights. Stable identity: a stored placement keeps `anchor` {centre (job X,
Y of its part's material centroid), area (mm2)}; on listing, each sheet's
parts take the stored placement whose anchor they fit best (centre within
max(0.35 * sqrt(area), 4 cells, 2 mm), area ratio 0.5 to 2; pairs taken
best first, the saved part number only breaking ties); placements without
an anchor (older files) go by part number. Placing or mating writes the
anchor. Each sheet's parts are cached in the kernel (keyed on its material,
the toolpath moves and tools) so a placement does not simulate again.
Piece frame: the box round the part, X and Y from its lower left corner, Z
up from the underside. Pose: the middle of that box at `position` (mm, Z
up), turned `rotation` degrees about X, then Y, then Z (fixed axes).
Unplaced pieces lie where their part is on the sheet, the sheets flat in a
row along X, 20 mm apart. A piece is `{sheet, part, component?}` (part defaults to 1).
- `assembly.addComponentPieces {components:[id], pose?:{position, rotation},
  at?:[x,y,z], beside?, quality?}` -> {revision, added}: undo step "Add
  Piece", saved; components already pieces are skipped, an unknown or flat
  component is refused. `pose` puts the first there as it is (the answer of
  `assembly.dropPlace`); else `at` stands the first unturned with the middle
  of its underside on the point; `beside` stands each (after that one)
  20 mm past every piece along X, centred on them along Y, on their lowest
  underside; with none of them a piece lies where its component lies on its
  sheet.
- `assembly.dropPlace {component, point:[x,y,z], onto?:{sheet, part,
  component}, quality?}` -> {position, rotation, revision}: reads only.
  Where a component let go at an assembly point lands (a piece already or
  not; a grounded one is refused): flush against the face of `onto` under
  the point (`pieces::face_at`: top, underside or a wall), else the ground
  (Z up). Of its part box's six sides the one facing most nearly against
  the face turns to face it (as a mate, the quarter turn about the face
  normal nearest how it stands) with its middle on the point; one already
  within 2 degrees of that keeps its turns. When it would then sink into
  any piece by more than 0.002 mm (`pieces::runs_into`: its top, underside
  and walls sampled just inside, both ways round), it slides along the
  face the least that clears every piece (four ways, never down, at most
  its own box diagonal), so it stays against the face's plane, and only
  when no slide clears moves out along the face normal: a coarse search on
  1 mm copies of the reliefs, then on along that way to where nothing is
  sunk in, halved to 0.001 mm (`assembly/drop.rs`). A new component is
  checked against its sheet's raw model piece as that will be without it
  (`PartsCache::model_leaving`), so it never meets its own material.
- `assembly.componentRelief {component}` -> blob (as `assembly.pieceRelief`,
  header also `component` and `size` [w, h, t]): the component standing
  alone cropped to its box, whether or not it is a piece (the ghost while
  placing it).
- `assembly.componentThumbnail {component, size?}` -> PNG blob (header
  `component`, `width`, `height`, section `png`): the relief shaded, 16 to
  1024 px (96 by default).
- `assembly.removeComponentPiece {component}` -> {revision}: drops it and its
  placement; undo step "Remove Piece".
- `assembly.pieces {quality?}` -> {components:[{id, name, sheet, sheetName,
  look}] (components that can still be added; `look`, 16 hex digits,
  changes when the component's shape does), pieces:[{sheet, part, component
  (null or the component id), componentName, parts (on the
  sheet), sheetName, vectorName (a named object on the sheet whose box
  matches the part's within max(3 cells, 8 mm), else null; null for a model
  piece), model (part 0, the raw model), position, rotation, size:[w,h,t]
  (the part's box), hasModel, hasToolpaths (of its sheet)}], revision}
- `assembly.pieceRelief {sheet, part, quality?}` -> BLOB like
  `sim.heightfield` (plus `sheet`, `part`), cropped to the part's box; every
  cell outside the part NaN.
- `assembly.placePiece {sheet, part, position, rotation}` -> {revision};
  undo step "Move Piece", saved. The mates are solved again in the same
  step.
- `assembly.solvePlace {sheet, part, component?, position, rotation,
  quality?}` -> {places:[{piece:{sheet,part,component?}, position,
  rotation}], conflicts, revision}: where every piece would end were the
  piece placed there, the mates solved; reads only (a drag shows it live).
- `assembly.verifyFit {target, tolerance, quality?, leaveOut?:[{sheet,
  part, component?}]}`: the joints between the pieces, less those named in
  `leaveOut` (the pieces the view does not show, when Include hidden pieces
  is cleared).
- `assembly.matePieces {target:{sheet,part,point}, moving:{sheet,part,point},
  offset?=0, flip?=false, turn?, quality?}` -> {revision, position, rotation,
  turn}. `flip` turns the moving piece over: the face opposite its picked
  face through the point (the underside below a top pick and the other way
  round, the far wall behind a wall pick, facing the other way) meets the
  target, so its other side faces the target . Points are in the piece frame (a pick on its surface). The face
  there is the top or underside where the point is at the material height
  (within 3/4 of a cell), else a wall facing away from the material round
  the point at its height (normals within 4 degrees of a sheet axis snap to
  it; the point moves onto the wall). The moving piece turns so its face
  looks at the target face and its point
  lands on the target point plus `offset` mm along the target normal.
  Round the normal a fixed direction of each face (local X on top and
  underside, the thickness on a wall) is lined up, then turned `turn`
  degrees; without `turn` the quarter turn closest to how the piece stands.
  Undo step "Mate Pieces", saved. A piece mated to itself is refused.
- Joint mates (kind `joint`, `joint` naming the Joint Creator joint) are
  made only by the `joints.*` commands: they hold the second piece where
  the joint puts it against the first, fixed in every direction. What they
  hold is worked out again from the joint and the two parts' job boxes
  each time the mates are taken up, and `assembly.pieces` (so every
  reader, Verify Fit too) moves pieces a joint mate no longer holds back
  together, so a toolpath edit or a joint change never leaves the parts
  apart. `matePieces` refuses kind `joint`; `editMate` never changes a
  joint mate's kind.
- `assembly.checkPieces {clearance, quality?}` -> {issues:[{a:{sheet,part},
  b:{sheet,part}, overlap(mm3), tooClose, bounds}], clearance}
- `assembly.pieceCutList {quality?}` -> {rows:[{sheet, part, parts,
  sheetName, vectorName, model, length, width, thickness}]}
- `assembly.pieceFace {sheet, part, point, quality?}` -> {face: {point,
  normal} | null}: the face `matePieces` would take at a point picked on
  the piece (for lighting it under the pointer).
- `assembly.pieceSource {sheet, model}` -> {revision}: the sheet's piece is
  its raw model (true) or its simulation's parts; undo step "Piece Source",
  saved (`modelSheets`). Refused for a sheet whose model is flat.

The vector part commands below have no UI since the Assembly View replaced
the Assembly tabs. The joinery commands (addJoint, updateJoint, removeJoint)
were removed in session 8; `joints` in older files is ignored.

## assembly.* (brief 017, functional part assembly)
Parts are flat solids built from closed vectors (outline, holes, pockets);
instances place them in assembly space (mm, Z up); mates put a child face
against a parent face, centres aligned, normals opposite, child turned a
half turn about the face's v axis. Faces: `{kind:"top"}`, `{kind:"bottom"}`,
`{kind:"edge",index:i}` (outline span i, counter-clockwise),
`{kind:"pocketFloor",index:k}`. Mesh face codes: 0 top, 1 bottom, 2+i edge i,
0x80000000+k pocket floor k, 0xFFFFFFFF not mateable.
- `assembly.get` -> {parts:[{part,size:[x,y,t],edges,instances,problem}],
  instances:[{instance,matrix(16, column major),mate}], mates, revision}
  (`part.frameOutline`, from files saved when joints could rewrite an
  outline, gives `size` and edges when present)
- `assembly.makePart {ids, name?, pockets?:[{vector,depth}], thickness?, grain?}`
  (largest closed vector is the outline, the rest holes unless pockets; adds
  one instance) -> {part, instance, revision}
- `assembly.updatePart {id, name?, thickness?(null = sheet), grain?, pockets?}`
- `assembly.removePart|removeInstance|unmate {id}`
- `assembly.addInstance {part, name?, position?, rotation?(deg XYZ)}` -> {id}
- `assembly.updateInstance {id, name?, position?, rotation?}`
- `assembly.mate {kind:"flush"|"hinge"|"slide", parent, parentFace, child,
  childFace, offset?:[u,v], angle?, gap?, axisPoint?, axisDir?, value?, min?, max?}`
  (hinge default axis: parent face u through its origin, 0..90 deg; slide:
  parent face u, 0..400 mm) -> {id}
- `assembly.updateMate {id, ...same fields}`
- `assembly.drive {id, value}` (hinge or slide, clamped, no undo step) -> {id, value}
- `assembly.meshes {explode?}` BLOB: header {parts:[{id,thickness}],
  instances:[{id,part,matrix}]}, sections p<id>.positions|normals (f32),
  p<id>.indices|faces (u32)
- `assembly.interference {step?, sweep?:{mate, steps?}}` -> {clashes:[{a,b,
  volume,bounds}], sweep:[{value, clashes}]}
- `assembly.cutList` -> {rows:[{part,name,quantity,length,width,thickness,sheet,grain}]}
  (length and width of the cut outline)

## joints.* (Joint Creator, session 8, agent joints)
Joints between flat parts, made on their closed vectors (module
`crates/carve-core/src/joints`). An edge is `{vector, from:[x,y], to:[x,y]}`
with the part on its left; part B is `{kind:"edge", vector, from, to}` or
`{kind:"face", edge, offset, along}` (a line on B's top face `offset` in
from B's `edge`, A's middle `along` from the edge's start). Kinds: finger,
rabbet, vJoint, crossLap, halfLap, dovetail, butterfly, puzzle (edge to
edge), livingHinge (one part: b is A's own edge), dado, tabSlot,
mortiseTenon, wedgedTenon, dowel, captiveNut, twistLock (face), and
cornerDovetail (edge to edge at a corner: tails on A as long as B is
thick, B's sockets with sloped walls as pockets of growing depth, 1 mm
steps at most, each as wide as the tail plus the clearance at the top of
its step, the narrowest cut through; B is cut outside face up; `count`,
`angle` 5 to 30, default 14; resolved `depth` is the step). dowel: the
marks on A are open paths (role mark) drawing each edge hole from the
edge in, named "drill mark N: D mm, E mm deep into the edge"; resolved
`length` is the dowel length. captiveNut: the bolt left out is the
smallest from M6 up whose bolt and nut slots (plus the clearance) are as
wide as the tool; one typed smaller is refused naming the smallest usual
bolt that fits. vJoint:
fingers for panels `angle` degrees apart (30 to 170, default 120), each
notch as deep as clears the other panel end on (both cut square); box
takes it too. twistLock: A's end through a hole in B across the line, a
neck (`width`, default A's thickness) with a head (`proud`) past B; the
hole is A's section long across the line with a round middle the neck's
diagonal plus the clearance across; A must be 2 mm wider than that round
middle. livingHinge: `pattern` "straight" (default) | "sinuous" |
"spiral", `radius` (inside bend radius, default 3 thicknesses) and
`angle` (bend, default 90) give the band (angle times radius plus half
the stock) unless `width` is given; spiral cells need about 2.5 times
(kerf + web) plus a kerf square. `joints.pickEdge` also answers `at`, where along the
edge the click was (crossLap takes it as atA and atB). Params (mm, all optional, worked out when left out): count,
width, depth, start ("finger"|"gap"), bias (0 to 0.5), blind, stop, proud,
ends, detent, angle, radius, pattern, diameter, inset, length, nutWidth, nutThickness, atA,
atB. Settings shared by every joint: fit ("slip" 0.2, "press" 0.1, "glue"
0.15, "knockDown" 0.25 mm a side, "custom" with customClearance), relief
("none"|"dogBone"|"tBone"), toolDiameter, thickness {sheet: measured mm}.
Kept in `assembly.jointSet` {settings, joints, originals (each touched
vector's path before any joint), nextId}; every change restores the
originals and applies every joint again, reusing the ids of the vectors the
joints made. Messages are plain words saying what to change.
- `joints.list` -> {settings, clearance, joints:[{id, kind, a, b, params,
  resolved, vectors:[{id, role}], bToA?, parts:[a, b], ok}], sheets:[{id,
  name, thickness, nominal}]}; role `{kind:"hole"}` | `{kind:"pocket",
  depth}` | `{kind:"part", thickness}` | `{kind:"mark"}`
- `joints.pickEdge {x, y, tolerance}` -> {edge: {vector, from, to, length,
  sheet, name, thickness, joint} | null, reason?}
- `joints.pickFace {x, y, a?}` -> {face: {edge, offset, along} | null,
  reason?}: the smallest closed vector around the point and its nearest
  straight edge; with `a`, A's middle goes where A fits.
- `joints.at {x, y, tolerance}` -> {joint}: the joint on an edge there.
- `joints.preview {kind, a, b, params, id?}` -> {outcome: {id, resolved,
  notes, bToA}, parts:[{vector, path}], made:[{vector, role, path}], madeIds,
  solid: {a:{rings, thickness}, b:{rings, thickness}, bToA}}: what add (or
  update of `id`) would make, in a step thrown away. bToA is B's part space
  (job X, Y, height above its underside) to A's, column major 4 by 4.
- Every change below also keeps the joints' Assembly View mates (`mates`
  in the reply: how many were added): the mate of a joint that is gone
  goes, and each joint whose two parts the toolpaths cut free as pieces
  gets a joint mate (ends anchored to the parts as other mates are), the
  mates solved so the second part goes to the first, in the same undo step.
- `joints.add {kind, a, b, params}` -> {id, outcome, mates, revision}; undo step
  "Add Joint". `joints.update {id, kind, a, b, params}` ("Edit Joint").
  `joints.remove {id}` ("Remove Joint") restores the edges exactly.
- `joints.setSettings {settings}` -> {joints, revision}: remakes every joint
  ("Joint Settings").
- `joints.box {ids, kind?, params?, dryRun?}` -> {ids?, pairs:[{a, b}]}:
  pairs the matching edges of the panels (same length, each pair of panels
  once, opposite panels never, closest first; the panel listed first is A)
  and adds a joint on each ("Box Joints"); dryRun only answers the pairs.
- `joints.makeToolpaths {tool}` -> {ids, replaced, mates}: per sheet, pockets by
  depth, holes (inside) and outlines and extra parts (outside), all named
  "Joints ..." and replacing an earlier run's ("Joint Toolpaths").
- `joints.assemble {id, quality?}` -> {mates}: keeps the joint's mate and
  solves the mates with A's piece leading, so B's piece goes against A's
  ("Assemble Joint"); refused until the toolpaths cut both parts free.
- `joints.coupon {kind, params}` -> {id, parts}: two small parts at a free
  spot on the active sheet and the joint between them ("Test Coupon").
- Parts a joint draws (role `part`: wedges, keys) go to the first free
  place on their sheet (clear of the other vectors by 40 mm) when first
  made and keep where they lie when the joints are made again, so they can
  be moved.

Later sections (post.*) are added by their briefs and
must follow the same conventions.
