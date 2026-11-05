// Icons that only menus, form buttons and panel row buttons draw: common
// commands (file, edit, view and help items that run no Design tab tool),
// the Mirror modes, the Dimension types and the Layers and Toolpaths list
// buttons. Loaded lazily by ToolIcons.tsx; drawn in the same 24 by 24 line
// style.

import type { AppCommand } from '../../lib/shortcuts';
import { SHEET } from './ToolIcons';

// Parts shared by several menu icons: a small eye over list rows, two
// guide lines, a square cut by a mirror axis, and a relief on a base line.
const EYE_TOP = 'M3 6.5s3.5-4.5 9-4.5 9 4.5 9 4.5-3.5 4.5-9 4.5-9-4.5-9-4.5Z;V:c12 6.5 1.8';
const EYE_TOP_SHUT =
  't:M3 6.5s3.5-4.5 9-4.5 9 4.5 9 4.5-3.5 4.5-9 4.5-9-4.5-9-4.5Z;R:M5 11.5 19 1.5';
const GUIDES = 'dtV:M7 2v20M2 7h20';
const MIRROR_BOX = 'r3 3 18 18';
const BASE = 'M2 20h20';

/** Five bars of rising height, the first `n` filled: a quality level. */
function qualityBars(n: number): string {
  return [0, 1, 2, 3, 4]
    .map((i) => `${i < n ? 'nV' : 't'}:r${2.5 + i * 4} ${16 - i * 3.5} 3 ${4 + i * 3.5}`)
    .concat('t:M1.5 22h21')
    .join(';');
}

export const EXTRA_ICONS: ReadonlyMap<string, string> = new Map<string, string>([
  ['snapSettings', 'R:M6 4v8a6 6 0 0 0 12 0V4h-3v8a3 3 0 0 1-6 0V4Z;M6 8h3M15 8h3'],
  // Space Selection: three objects spread with equal gaps.
  [
    'spaceHorizontal',
    'nV:r2 5 4 10;nV:r10 3 4 14;nV:r18 6 4 8;t:M6 20h4M14 20h4M6 18.5v3M10 18.5v3M14 18.5v3M18 18.5v3',
  ],
  [
    'spaceVertical',
    'nV:r5 2 10 4;nV:r3 10 14 4;nV:r6 18 8 4;t:M20 6v4M20 14v4M18.5 6h3M18.5 10h3M18.5 14h3M18.5 18h3',
  ],
  // Menu commands: file, edit, view and help items that run no Design tab
  // tool, drawn in the same style so every menu keeps one look.
  ['fileNewTemplate', 'M6 3h8l4 4v14H6ZM14 3v4h4;tdV:r9 11 6 7'],
  ['fileRecent', 'M6 3h8l4 4v14H6ZM14 3v4h4;tV:M9 11h6M9 14h6M9 17h4'],
  ['fileSaveAs', 'M3 3h12l3 3v4M3 3v15h7M7 3v4h6V3;V:m13 21 .75-3 6-6 2.25 2.25-6 6Z'],
  ['folder', 's:M3 6h6l2 2h10v11H3Z'],
  // A bookcase: a carcass with two shelves and books on them.
  // A panel seen flat with a pencil on it: edit a face in the 2D view.
  ['faceEdit2d', 'r3 5 14 14;V:m13 21 .75-3 6-6 2.25 2.25-6 6Z'],
  // A panel seen in 3D with its top face lit: edit a face in the 3D view.
  ['faceEdit3d', 'M3 15l8 4 10-5-8-4Z;M3 15v2l8 4 10-5v-2;nV:M5 15l6 3 8-4-6-3Z'],
  [
    'furniture',
    'M5 3h14v18H5Z;M5 9.5h14M5 15h14;nV:r7 5 2 4.5;nV:r9.5 5.5 2 4;nV:r7 11 2.5 4;t:M5 21v1M19 21v1',
  ],
  ['print', 'M7 9V3h10v6M6 17H3V9h18v8h-3;V:M7 14h10v7H7Z'],
  ['printPreview', 'M5 3h9l4 4v4M5 3v18h7;V:c16 16 3.5;m18.5 18.5 2.5 2.5'],
  ['exportVectors', 'tV:M4 20c3-8 6-10 10-10;f:c4 20 1.5;f:c14 10 1.5;M17 11V3M14 6l3-3 3 3'],
  ['importToolpaths', 'tP:M3 21v-7h4v7h4v-7;M17 3v8M14 8l3 3 3-3'],
  ['editCut', 'R:c6.5 18 2.5;R:c17.5 18 2.5;M8.2 16.2 17 3M15.8 16.2 7 3'],
  ['editCopy', 't:M15 7V3H4v13h4;V:M9 8h11v13H9Z'],
  ['editPaste', 'M8 5H5v16h14V5h-3;V:M9 3h6v4H9Z;t:M8 12h8M8 15h8M8 18h5'],
  ['editDelete', 'R:m6 6 12 12M18 6 6 18'],
  ['selectAll', 'dtS:r2.5 2.5 19 19;M8 6v11l3-3 2 4.5 1.8-.9-2-4.4H17Z'],
  ['notes', 'M5 3h14v18H5Z;tV:M8 8h8M8 12h8M8 16h5'],
  [
    'variables',
    'M8 4C6 4 6 5 6 7v2c0 2-1 3-2 3 1 0 2 1 2 3v2c0 2 0 3 2 3M16 4c2 0 2 1 2 3v2c0 2 1 3 2 3-1 0-2 1-2 3v2c0 2 0 3-2 3;tV:m10 9 4 6M14 9l-4 6',
  ],
  ['options', 't:M3 6h18M3 12h18M3 18h18;nV:r6.5 4 3 4;nV:r14.5 10 3 4;nV:r9.5 16 3 4'],
  ['zoomJob', 'c10 10 7;m15 15 6 6;tV:r6.5 7.5 7 5'],
  ['zoomSelection', 'c10 10 7;m15 15 6 6;tdS:r6.5 7.5 7 5'],
  ['saveImage', 'r3 5 18 14;tG:m3 17 5-5 4 4 3-3 6 6;fA:c16 9 1.5'],
  ['help', 'c12 12 9;V:M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.7v.5;fV:c12 17 1.1'],
  ['machine', 'M2 20h20M5 20V5h14v15;V:M10 5h4v5h-4ZM12 10v4'],
  ['post', 'M6 3h8l4 4v14H6ZM14 3v4h4;tP:m10.5 11-2 2.5 2 2.5M13.5 11l2 2.5-2 2.5'],
  // Machine menu: an offline package (a box with an arrow in), a search for
  // a machine online (a globe under a lens) and the post-processor
  // database brought up to date.
  [
    'machinePackage',
    'w:M4 10.5 12 7l8 3.5v7.5L12 21.5 4 18Z;t:m4 10.5 8 3.5 8-3.5M12 14v7.5;G:M12 1v4.5M10 3.5l2 2 2-2',
  ],
  ['searchOnline', 'c10 10 7;m15 15 6 6;tV:e10 10 3 7;tV:M3 10h14'],
  [
    'postDatabase',
    'e10 5 6 2;M4 5v12c0 1.1 2.7 2 6 2M16 5v4;t:M4 11c0 1.1 2.7 2 6 2;G:M21 16a4 4 0 1 1-1.2-2.8M20 11v2.5h-2.5',
  ],
  // Gadgets: a puzzle piece, one with a plus to install, and a keyboard
  // for shortcuts.
  ['gadget', 'G:M5 9h3.5a2 2 0 1 1 4 0H16v3.5a2 2 0 1 1 0 4V20H5Z'],
  ['gadgetInstall', 'G:M4 10h3.5a2 2 0 1 1 4 0H15v3.5a2 2 0 1 1 0 4V21H4Z;G:M19 2v6M16 5h6'],
  ['keyboard', 'r2 6 20 12 2;tV:M6 10h1M10 10h1M14 10h1M18 10h1M7 14h10'],
  // Help: videos, what is new, release notes, licenses and the license
  // key, the online FAQ, forum, support and user portal, the post
  // processor guide, migrating settings, updates, the start up guide and
  // About.
  ['video', 'c12 12 9;fR:M10 8.5v7l6-3.5Z'],
  [
    'whatsNew',
    'Ms:M10 3c.8 4.5 2.5 6.2 7 7-4.5.8-6.2 2.5-7 7-.8-4.5-2.5-6.2-7-7 4.5-.8 6.2-2.5 7-7Z;G:M19 15v6M16 18h6',
  ],
  [
    'releaseNotes',
    'M6 3h8l4 4v14H6ZM14 3v4h4;t:M11 11h4M11 14.5h4M11 18h4;fV:c8.75 11 .9;fV:c8.75 14.5 .9;fV:c8.75 18 .9',
  ],
  ['licenses', 'M4 3h16v12H4Z;t:M7 7h10M7 10h6;R:c16 15 2.5;R:M14.5 17 14 21l2-1 2 1-.5-4'],
  ['licenseKey', 'A:c8 15 4.5;A:M11.2 11.8 20 3M16.5 6.5l2.5 2.5M14 9l2 2'],
  ['faq', 'M3 4h18v12h-9l-5 4v-4H3Z;V:M9.8 8.2a2.2 2.2 0 1 1 3 2c-.5.2-.8.6-.8 1.2;fV:c12 13.6 .9'],
  ['forum', 'V:M2 3h13v9H8l-4 3v-3H2Z;M17 8h5v9h-2v3l-4-3H9v-3'],
  [
    'support',
    'c12 12 9;c12 12 4;tR:m5.6 5.6 3.6 3.6M14.8 14.8l3.6 3.6M18.4 5.6l-3.6 3.6M9.2 14.8l-3.6 3.6',
  ],
  ['userPortal', 'V:c12 8 4;V:M4 21c0-4.4 3.6-7 8-7s8 2.6 8 7'],
  [
    'postGuide',
    'M2 5h7a3 3 0 0 1 3 3v12a2 2 0 0 0-2-2H2ZM22 5h-7a3 3 0 0 0-3 3v12a2 2 0 0 1 2-2h8Z;tP:M6 9.5 4.5 11 6 12.5M8 9.5l1.5 1.5L8 12.5',
  ],
  ['migrate', 'td:r2 7 7 10;V:r15 7 7 10;M9 12h6M13 10l2 2-2 2'],
  ['checkUpdates', 'G:M20 12a8 8 0 1 1-2.34-5.66;G:M20 3v5h-5;t:M12 8v7M9.5 12.5 12 15l2.5-2.5'],
  [
    'kickstarter',
    'M12 2c3 2.5 4.5 6.5 3.5 12h-7C7.5 8.5 9 4.5 12 2Z;V:c12 8.5 1.6;R:M8.5 12 5 15.5V19l3.8-2.2M15.5 12l3.5 3.5V19l-3.8-2.2;t:M11 17v4M13 17v4',
  ],
  ['about', 'c12 12 9;V:M12 11v6;fV:c12 7.5 1.1'],
  // Document variables: a new one (braces round a plus); a variable in
  // use is the variables icon.
  [
    'variableNew',
    'M8 4C6 4 6 5 6 7v2c0 2-1 3-2 3 1 0 2 1 2 3v2c0 2 0 3 2 3M16 4c2 0 2 1 2 3v2c0 2 1 3 2 3-1 0-2 1-2 3v2c0 2 0 3-2 3;tG:M12 9v6M9 12h6',
  ],
  // Segment Model tree: delete a segment's children (a parent over two
  // crossed out children) and separate merged segments (two parts pulled
  // apart).
  [
    'segmentDeleteChildren',
    'M:r9 2.5 6 4.5;t:M12 7v3.5M6.5 10.5h11M6.5 10.5V13M17.5 10.5V13;dt:r3.5 13 6 6;dt:r14.5 13 6 6;R:m4.5 14 4 4M8.5 14l-4 4M15.5 14l4 4M19.5 14l-4 4',
  ],
  [
    'segmentSeparate',
    'Ms:r2.5 8 6.5 11;Ms:r15 8 6.5 11;dt:M12 3v19;M9 5H4.5M6 3.5 4.5 5 6 6.5M15 5h4.5M18 3.5 19.5 5 18 6.5',
  ],
  ['visible', 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z;V:c12 12 3'],
  ['rename', 't:M14 8H3v8h11;V:M18 5v14M16 5h4M16 19h4'],
  ['properties', 't:M9 7h11M9 12h11M9 17h11;fV:c5 7 1.25;fV:c5 12 1.25;fV:c5 17 1.25'],
  ['lock', 'M7 11V8a5 5 0 0 1 10 0v3;A:r5 11 14 10'],
  ['layers', 'V:m12 4-9 5 9 5 9-5Z;t:m3 13 9 5 9-5'],
  // Form buttons: Mirror modes (a shape and its mirror image about an
  // axis, the selection's edge drawn bold), Dimension types, and the list
  // row buttons of the Layers and Toolpaths panels.
  ['mirrorVertical', 'dt:M2 12h20;nV:M6 10 18 3v7Z;R:m6 14 12 7v-7Z'],
  ['mirrorLeft', 't:r12 5 9 14;nV:M14 17 19 7v10Z;tR:M10 17 5 7v10Z;M12 3v18'],
  ['mirrorRight', 't:r3 5 9 14;nV:M10 17 5 7v10Z;tR:M14 17 19 7v10Z;M12 3v18'],
  ['mirrorTop', 't:r5 12 14 9;nV:M17 14 7 19h10Z;tR:M17 10 7 5h10Z;M3 12h18'],
  ['mirrorBottom', 't:r5 3 14 9;nV:M17 10 7 5h10Z;tR:M17 14 7 19h10Z;M3 12h18'],
  ['mirrorMaterialX', `${SHEET};dt:M12 5v14;nV:M10 9 6 16h4Z;R:m14 9 4 7h-4Z`],
  ['mirrorMaterialY', `${SHEET};dt:M5 12h14;nV:m9 10 7-4v4Z;R:m9 14 7 4v-4Z`],
  ['dimLinear', 't:m3 15 6 6M15 3l6 6;V:M6 18 18 6M6.5 14.5 6 18l3.5-.5M14.5 6.5 18 6l-.5 3.5'],
  ['dimVertical', 't:M8 4h11M8 20h11;V:M14 4v16M12 7l2-3 2 3M12 17l2 3 2-3'],
  ['dimAngular', 't:M3 20h18M3 20 15 5;V:M14 20a11 11 0 0 0-4.1-8.6'],
  ['dimArc', 't:c11 13 8;f:c11 13 1.25;V:m11 13 6.5-6.5M14 7l3.5-.5L17 10'],
  ['hidden', 't:M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z;R:m4 20 16-16'],
  ['unlock', 'M7 11V8a5 5 0 0 1 9.6-2;A:r5 11 14 10'],
  ['arrowUp', 'M12 20V5M6 11l6-6 6 6'],
  ['arrowDown', 'M12 4v15M6 13l6 6 6-6'],
  ['moveToLayer', 'tV:m12 10-9 5 9 5 9-5Z;M12 3v9M9 9l3 3 3-3'],
  // Layers panel toolbar: add a layer (a plus beside the stack) and hide
  // empty layers (a dashed, empty layer crossed out under a full one).
  ['layerAdd', 'V:m10 4-8 4.5 8 4.5 8-4.5Z;t:m2 12.5 8 4.5 8-4.5;G:M19 14v7M15.5 17.5h7'],
  ['layerHideEmpty', 'nV:m10 3-8 4.5 8 4.5 8-4.5Z;dt:m2 12 8 4.5 8-4.5;R:m16 15 5 5M21 15l-5 5'],
  // Sheets panel: a material sheet with a plus, a pencil or a cross.
  ['sheetAdd', 'M3 5h9l3 3v13H3Z;G:M19 2v7M15.5 5.5h7'],
  ['sheetEdit', 'M3 5h9l3 3v4M3 5v16h7;V:m12 21 .75-3 6.5-6.5 2.25 2.25-6.5 6.5Z'],
  ['sheetDelete', 'M3 5h9l3 3v13H3Z;R:m16.5 3 5 5M21.5 3l-5 5'],
  // Set Size modes: the shapes scaled as one (one dashed box round them
  // all) or each scaled in its own dashed box.
  ['sizeAsOne', 'dt:r2 5 20 14;V:r4.5 9 5 7;V:M13 16l2.5-7 2.5 7Z'],
  [
    'sizeIndividually',
    'dt:r3 7.5 7.5 10;dt:r12.5 7.5 8.5 10;V:r4.5 9 4.5 7;V:M13.5 16l3.25-7L20 16Z',
  ],
  ['materialSetup', 'w:M3 8l9-4 9 4v9l-9 4-9-4Z;t:M3 8l9 4 9-4M12 12v9'],
  // Joint Creator buttons: Create (a check), Test Coupon (two small
  // samples), Box from Selection (a box with fingers on its corner),
  // Close, and the corner reliefs (an inside corner, with a dog-bone and
  // with a T-bone).
  ['jointCreate', 'G:m4 12.5 5 5L20 6'],
  ['jointCoupon', 'w:r3 7 7 10;w:r14 7 7 10;t:M10 10h4M10 14h4'],
  ['jointBox', 'w:M3 8l9-4 9 4v9l-9 4-9-4Z;t:M3 8l9 4 9-4M12 12v9;nV:M12 12l3-1.4v3l-3 1.4Z'],
  ['dismiss', 'm6 6 12 12M18 6 6 18'],
  // Form action buttons: Calculate (a calculator), Apply and OK (a check),
  // Create (a plus), Preview (an eye), Reset (an arrow turning back) and
  // Run (a play mark).
  ['formCalculate', 't:r5 2 14 20 2;V:r8 5 8 4;M9 13h0M12 13h0M15 13h0M9 17h0M12 17h0M15 17h0'],
  ['formApply', 'G:m4 12.5 5 5L20 6'],
  ['formCreate', 'G:M12 5v14M5 12h14'],
  ['formPreview', 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z;V:c12 12 3'],
  ['formReset', 'V:M4 12a8 8 0 1 0 2.4-5.7;V:M4 4v4.5h4.5'],
  ['formRun', 'G:M7 4v16l13-8Z'],
  // Text buttons in forms and panels: remove (a minus), edit (a pencil),
  // browse for a file (an open folder), export to a file (a tray with an
  // arrow out), swap, the arrows that turn or step back, playback, a new
  // group (a folder with a plus), pick in the view (a cross hair) and set a
  // value (a pin in a field).
  ['itemRemove', 'R:M5 12h14'],
  ['itemEdit', 'M4 20h4L19 9l-4-4L4 16Z;t:M13 7l4 4'],
  ['exportFile', 'M3 14v6h18v-6;M12 14V3M8 7l4-4 4 4'],
  ['swap', 'M4 8h15M16 5l3 3-3 3;M20 16H5M8 13l-3 3 3 3'],
  ['arrowLeft', 'M20 12H5M11 6l-6 6 6 6'],
  ['arrowRight', 'M4 12h15M13 6l6 6-6 6'],
  ['turnLeft', 'M9 5 4 10l5 5;M4 10h9a7 7 0 0 1 7 7v2'],
  ['turnRight', 'M15 5l5 5-5 5;M20 10h-9a7 7 0 0 0-7 7v2'],
  ['turnUp', 'M5 9l5-5 5 5;M10 4v9a7 7 0 0 0 7 7h2'],
  ['turnDown', 'M5 15l5 5 5-5;M10 20v-9a7 7 0 0 1 7-7h2'],
  ['layFlat', 'M3 20h18;Ms:r6 14 12 4;t:M12 3v8M9 8l3 3 3-3'],
  ['play', 'fG:M7 4v16l13-8Z'],
  ['pause', 'f:r6 5 4 14;f:r14 5 4 14'],
  ['stepForward', 'fG:M5 5v14l10-7Z;M18 5v14'],
  ['folderAdd', 's:M3 6h6l2 2h10v11H3Z;tG:M12 10.5v6M9 13.5h6'],
  ['pickPoint', 'c12 12 6;M12 2v5M12 17v5M2 12h5M17 12h5;fR:c12 12 1.25'],
  ['setValue', 'r3 8 18 8 1;t:M7 12h6;V:M17 3v6M15 7l2 2 2-2'],
  ['reliefNone', 'P:M6 4v14h14;dt:M9 4v11h11'],
  ['reliefDogBone', 'P:M6 4v11a3 3 0 1 0 3 3h11;dt:M9 4v11h11'],
  ['reliefTBone', 'P:M6 4v14a3 3 0 0 0 6 0h8;dt:M9 4v11h11'],
  // Living hinge patterns: staggered straight slots, wavy slots, spirals.
  ['hingeStraight', 'w:r3 5 18 14;tP:M8 7v4M8 13v4M11 9v6M14 7v4M14 13v4M17 9v6'],
  [
    'hingeSinuous',
    'w:r3 5 18 14;tP:M8 7q1.5 2 0 4M8 13q1.5 2 0 4M12 8q1.5 2 0 4t0 4M16 7q1.5 2 0 4M16 13q1.5 2 0 4',
  ],
  [
    'hingeSpiral',
    'w:r3 5 18 14;tP:M8 12a1 1 0 1 1 1.5 1.5a2.6 2.6 0 1 1-3.5-3.5M16 12a1 1 0 1 0-1.5 1.5a2.6 2.6 0 1 0 3.5-3.5',
  ],
  // Preview Toolpaths checks: the check colors on a carved surface, the
  // tolerance (plus or minus), and one icon for each kind of issue.
  ['checkColors', 't:M3 20h18;M3 15c3-6 6-6 9-2s6 3 9-3;fG:c8 15.5 1.6;fR:c16.5 13 1.6'],
  ['tolerance', 'V:M12 4v8M8 8h8;M8 15h8;t:M4 20h16'],
  ['issueRapid', 'w:r3 12 18 9;dR:M3 10h13;R:M15 7l3 3-3 3'],
  ['issuePlunge', 'w:r3 12 18 9;R:M12 2v12M9 11l3 3 3-3'],
  ['issueFlute', 'w:r3 9 6 12;w:r15 9 6 12;A:M10 3h4v16h-4Z;tA:M10 11h4'],
  ['issueShank', 'w:r3 11 7 10;w:r14 11 7 10;M10.5 13h3v7h-3Z;A:M9 3h6v10H9Z'],
  ['issueHolder', 'w:r3 15 18 6;M11 10h2v8h-2Z;A:M7 3h10v7H7Z'],
  ['issueClamp', 'w:r3 15 18 6;A:M4 9h9v3H4ZM6 12v3;M17 3v10M15 11l2 2 2-2'],
  ['issueGouge', 't:M3 8v12h18V8;M3 8h18;nR:M8 8q4 9 8 0Z'],
  ['issueLeftover', 't:M3 8v12h18V8;M3 14h18;nA:M8 14q4-7 8 0Z'],
  ['issueUndercut', 't:M3 20h18;w:M3 9h18v5H3Z;R:M8 14v6M16 14v6'],
  // Menu items that were blank: file, selection, sides, component and
  // display toggles (the check mark stays beside the icon), guides,
  // windows, and the right click menus of the 2D view, node editing, the
  // layer, sheet, toolpath and component lists.
  ['clearRecent', 'M6 3h8l4 4v6M6 3v18h7M14 3v4h4;t:M9 11h6M9 14h4;R:m15 16 5 5M20 16l-5 5'],
  ['fileClose', 'M6 3h8l4 4v14H6ZM14 3v4h4;R:m9.5 12 5 5M14.5 12l-5 5'],
  [
    'printSetup',
    'M7 9V3h10v6M6 17H3V9h18v8h-3;t:M8 14h8M8 17.5h8M8 21h8;fV:c10.5 14 1.3;fV:c13.5 17.5 1.3;fV:c10 21 1.3',
  ],
  ['selectOpen', 'dtS:r2.5 2.5 19 19;V:M6 17c2-8 6-10 12-10;f:c6 17 1.5;f:c18 7 1.5'],
  ['selectDuplicates', 'dtS:r2.5 2.5 19 19;r6 6 8 8;nV:r10 10 8 8'],
  ['selectLayer', 'dtS:r2.5 2.5 19 19;V:m12 6-6 3.5 6 3.5 6-3.5Z;t:m6 13 6 3.5 6-3.5'],
  ['selectNone', 'dtS:r2.5 2.5 19 19;R:m8 8 8 8M16 8l-8 8'],
  [
    'swapSides',
    'w:r3 10 18 4;M6 7c2-3 10-3 12 0M16 4.5 18 7l-2.8.8M18 17c-2 3-10 3-12 0M8 19.5 6 17l2.8-.8',
  ],
  ['zeroPlane', 'Ms:M2 16 7 10h15l-5 6Z;t:M4 20h16'],
  ['fromPreview', `${BASE};tw:M3 20v-4h4c1 3 3 3 4 0h10v4;M11 3h2v7h-2Z;M12 10v3`],
  ['toolpathsTab', 'r3 4 18 16;M15 4v16;tP:M17 8h2M17 11h2M17 14h2'],
  ['toolpathDrawing', 't:r3 3 18 18;P:M6 7h12v3.3H6v3.4h12V17H6'],
  ['preview2d', 'P:r6 6 12 12 3;t:r3 3 18 18 5'],
  ['previewSolid', 'nP:M3 3h18v18H3ZM7 7v10h10V7Z;P:M3 3h18v18H3ZM7 7h10v10H7Z'],
  ['autoOpen3d', 'w:M4 8l6-3 6 3v7l-6 3-6-3Z;t:M4 8l6 3 6-3M10 11v7;M16 21l5-5M17.5 16H21v3.5'],
  ['moveRapid', 'dR:M3 12h15;R:M16 8.5 19.5 12 16 15.5;t:M3 19h18'],
  ['movePlunge', 'w:r3 15 18 6;P:M12 3v13M9 13l3 3 3-3'],
  ['moveRetract', 'w:r3 15 18 6;P:M12 16V3M9 6l3-3 3 3'],
  ['moveJoin', 'M3 20V10M21 20V10;dP:M3 10q9-8 18 0'],
  ['previewQuality', 'M3 17a9 9 0 0 1 18 0;R:M12 17l5-5.5;t:M5 17h2M17 17h2M12 8v2M6.5 11.5 8 13'],
  ['quality1', qualityBars(1)],
  ['quality2', qualityBars(2)],
  ['quality3', qualityBars(3)],
  ['quality4', qualityBars(4)],
  ['quality5', qualityBars(5)],
  ['guides', 'r2 2 20 4;t:M6 4v2M10 4v2M14 4v2M18 4v2;dtV:M8 6v16M2 14h20'],
  ['guidesVisible', `${GUIDES};M9 16s2.6-4 6.5-4 6.5 4 6.5 4-2.6 4-6.5 4S9 16 9 16Z;c15.5 16 1.6`],
  ['guidesDelete', `${GUIDES};R:m12 12 8 8M20 12l-8 8`],
  ['guidesLock', `${GUIDES};M13 15v-2a3 3 0 0 1 6 0v2;A:r11.5 15 9 6`],
  ['guidesUnlock', `${GUIDES};M13 15v-2a3 3 0 0 1 5.8-1;A:r11.5 15 9 6`],
  ['origin', 'M5 19V4M5 19h15M3 6l2-2 2 2M18 17l2 2-2 2;fR:c5 19 1.75'],
  ['modelingPlane', 'Ms:M2 17 7 9h15l-5 8Z;t:M4.5 13h15M7 17l5-8M12 17l5-8'],
  ['materialBlock', 'w:M3 10l9-4 9 4v5l-9 4-9-4Z;t:M3 10l9 4 9-4M12 14v5'],
  ['multiSided', 'w:r3 4 18 6;w:r3 14 18 6;dt:M7 10v4M17 10v4'],
  [
    'autoWrap',
    'w:e12 6 7 2.5;w:M5 6v12c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5V6;t:M5 12c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5',
  ],
  ['colorShaded', `${BASE};Ms:M3 20c2-7 5-10 9-10s7 3 9 10Z;fA:c17 5 2`],
  ['shadedBackground', 'r3 4 18 16;nV:M3 14h18v6H3Z;t:M3 9h18'],
  [
    'lightFollows',
    'A:c8 8 3;tA:M8 2v1.5M8 12.5V14M2 8h1.5M12.5 8H14M3.8 3.8l1 1M11.2 11.2l1 1M3.8 12.2l1-1M11.2 4.8l1-1;M12 17s2-3 5-3 5 3 5 3-2 3-5 3-5-3-5-3Z;c17 17 1.2',
  ],
  ['shadowShading', 'Ms:c10 10 6;n:e14 19.5 7 2;t:M7 8a4 4 0 0 1 3-2'],
  // Tile Vertical sets the views side by side, Tile Horizontal one above
  // the other.
  ['tileVertical', 'V:r2 4 9 16;r13 4 9 16'],
  ['tileHorizontal', 'V:r3 2 18 9;r3 13 18 9'],
  ['cascade', 't:r2 3 12 9;t:r5 7 12 9;V:r9 11 12 9'],
  ['selectInside', 'r2.5 2.5 19 19 3;tdV:c9 12 3;tdV:r13 9 5 6'],
  [
    'exclusiveOr',
    't:c9 12 6;t:c15 12 6;nV:M12 6.8A6 6 0 1 0 12 17.2A6 6 0 0 1 12 6.8Z;nV:M12 6.8A6 6 0 1 1 12 17.2A6 6 0 0 0 12 6.8Z',
  ],
  [
    'adjustText',
    'M4 9h9M8.5 9v9;tS:M3 21h11M5 19.5 3 21l2 1.5M12 19.5l2 1.5-2 1.5;V:M15 4a8 8 0 0 1 6 8',
  ],
  ['textOffCurve', 'M5 4h8M9 4v8;tV:M3 19c3-3 6-3 9 0s6 3 9 0;M18 13V4M15.5 6.5 18 4l2.5 2.5'],
  ['nodeSmooth', 't:M3 18 12 6l9 12;V:M3 18c3-7 6-11 9-11s6 4 9 11;fS:c12 7 1.6'],
  ['nodeStart', 'V:r4 5 16 14 2;fS:c4 12 2.2;t:M8 9.5l2.5 2.5L8 14.5'],
  [
    'nodeAlignX',
    'dt:M12 2v20;fS:c12 5 1.75;fS:c12 12 1.75;fS:c12 19 1.75;t:M5 12h4M7.5 10.5 9 12l-1.5 1.5',
  ],
  [
    'nodeAlignY',
    'dt:M2 12h20;fS:c5 12 1.75;fS:c12 12 1.75;fS:c19 12 1.75;t:M12 5v4M10.5 7.5 12 9l1.5-1.5',
  ],
  ['reverseDirection', 'V:M4 8h15M16 5l3 3-3 3;t:M20 16H5M8 13l-3 3 3 3'],
  [
    'nodeMirrorH',
    'dt:M12 2v20;V:M10 4c-5 2-5 14 0 16M14 4c5 2 5 14 0 16;fS:c6.3 12 1.4;fS:c17.7 12 1.4',
  ],
  [
    'nodeMirrorV',
    'dt:M2 12h20;V:M4 10c2-5 14-5 16 0M4 14c2 5 14 5 16 0;fS:c12 6.3 1.4;fS:c12 17.7 1.4',
  ],
  ['tangency', 'tV:M3 18c4-10 14-10 18 0;M4 10.5h16;fS:c12 10.5 1.6;S:r2.5 9 3 3;S:r18.5 9 3 3'],
  ['nodeInsert', 'V:M3 16 21 8;fS:c12 12 1.8;t:M12 2v5M9.5 4.5h5'],
  [
    'nodeMidpoint',
    'V:M3 16 21 8;f:c3 16 1.4;f:c21 8 1.4;S:c12 12 2.2;t:M7 20h10M7 18.5v3M17 18.5v3',
  ],
  ['spanLine', 'V:M4 18 20 6;f:c4 18 1.6;f:c20 6 1.6'],
  [
    'spanBezier',
    'V:M4 18C4 8 20 16 20 6;t:M4 18V8M20 6v10;fS:c4 18 1.6;fS:c20 6 1.6;S:r2.5 6.5 3 3;S:r18.5 14.5 3 3',
  ],
  ['layerActive', 'nV:m10 4-8 4.5 8 4.5 8-4.5Z;t:m2 12.5 8 4.5 8-4.5;G:M15 18l2.5 2.5L22 15.5'],
  ['layerMerge', 't:m12 3-9 4.5 9 4.5 9-4.5Z;nV:m12 13-9 4.5 9 4.5 9-4.5Z;M12 8v6M10 12l2 2 2-2'],
  ['layerInsert', 'V:m12 12-9 4.5 9 4.5 9-4.5Z;G:M12 2v7M8.5 5.5h7'],
  ['sheetActive', 'M3 5h9l3 3v13H3Z;G:M16 16l2 2 4-5'],
  ['sheetMoveHere', 'M3 9h9l3 3v9H3Z;tdV:r15 2 6 5;M18 7v4a4 4 0 0 1-4 4h-4M12 13l-2 2 2 2'],
  ['moveToSheet', 'M8 5h9l3 3v13H8Z;V:M2 13h9M8 10l3 3-3 3'],
  // Assembly View Add Component: a block with a plus (add beside the
  // others), a pointer over a block's ghost (click to place) and a hand
  // dragging a block (drag from the Components tab).
  ['pieceAdd', 'w:r3 11 12 9;t:M3 11l3-3h12l-3 3M15 20l3-3V8;G:M19 2v6M16 5h6'],
  ['piecePlace', 'dtS:r3 11 12 9;M14 3v11l3-3 2.5 5 2-1-2.5-5h4Z'],
  ['pieceCancel', 'R:M6 6l12 12M18 6 6 18'],
  ['pieceDrag', 'w:r12 3 9 7;t:M4 21v-6a2 2 0 0 1 4 0v-3a2 2 0 0 1 4 0v5;dt:M12 10l-3 3'],
  ['showOnly', `${EYE_TOP};nP:r3 14 18 3.5;t:M3 20.5h18`],
  ['showAllBut', `${EYE_TOP};dtP:r3 14 18 3.5;M3 20.5h18`],
  ['showSameTool', `${EYE_TOP};M10 13h4v4l-2 4.5-2-4.5Z`],
  ['showAll', `${EYE_TOP};P:M3 15h18M3 18.5h18M3 22h18`],
  ['hideAll', `${EYE_TOP_SHUT};tP:M3 15h18M3 18.5h18M3 22h18`],
  ['deleteHidden', `${EYE_TOP_SHUT};R:m9 15 6 6M15 15l-6 6`],
  ['deleteVisible', `${EYE_TOP};R:m9 15 6 6M15 15l-6 6`],
  [
    'recalculateVisible',
    'G:M20 12a8 8 0 1 1-2.34-5.66;G:M20 3v5h-5;t:M7 12s2-3 5-3 5 3 5 3-2 3-5 3-5-3-5-3Z;f:c12 12 1.2',
  ],
  ['groupVisible', 'dtS:r2.5 2.5 19 19;M6 12s2.5-4 6-4 6 4 6 4-2.5 4-6 4-6-4-6-4Z;V:c12 12 1.8'],
  ['treeExpand', 't:r3 3 18 18 2;M8 12h8M12 8v8'],
  ['treeCollapse', 't:r3 3 18 18 2;M8 12h8'],
  // Component combine modes: a relief added, subtracted, the higher or
  // the lower kept where two meet, or multiplied.
  ['combineAdd', `${BASE};Ms:M4 20c2-9 10-9 12 0Z;G:M18 4v6M15 7h6`],
  ['combineSubtract', 'M2 12h20;Ms:M4 12c2 9 10 9 12 0Z;R:M15 5h6'],
  ['combineHigh', `${BASE};t:M3 20c2-8 8-8 10 0;Ms:M9 20c2-13 9-13 11 0;t:M5 3v6M3 5l2-2 2 2`],
  ['combineLow', `${BASE};Ms:M3 20c2-8 8-8 10 0;t:M9 20c2-13 9-13 11 0;t:M5 3v6M3 7l2 2 2-2`],
  ['combineMultiply', `${BASE};Ms:M4 20c2-9 10-9 12 0;m15.5 4.5 5 5M20.5 4.5l-5 5`],
  // Component edits the tree offers that no tool icon shows.
  ['componentTilt', `${BASE};Ms:M3 20v-5l18-7v12Z;t:M6 7a6 6 0 0 1 6-2.5M10.5 3 12 4.5 10.5 6`],
  ['componentFade', `${BASE};Ms:M3 20V7c6 0 12 7 18 13Z`],
  ['componentFlip', 'dt:M2 12h20;Ms:M4 12c2-7 14-7 16 0Z;tM:M4 12c2 7 14 7 16 0'],
  [
    'otherSideCopy',
    'w:r3 3 18 5;dt:r3 16 18 5;M12 8v7M9.5 12.5 12 15l2.5-2.5;tG:M17 10.5h4M19 8.5v4',
  ],
  ['otherSideMove', 'dt:r3 3 18 5;w:r3 16 18 5;M12 8v7M9.5 12.5 12 15l2.5-2.5'],
  ['clip', `${BASE};dtR:M6 4v16M18 4v16;Ms:M6 20v-6c2-3 4-4 6-4s4 1 6 4v6`],
  // Level mirror modes: the half or quarter copied over (tinted) and
  // the axis it is mirrored across.
  ['mirrorNone', `${MIRROR_BOX};tV:M7 17c2-6 4-9 10-10`],
  ['mirrorLeftToRight', `${MIRROR_BOX};dt:M12 3v18;nM:r3 3 9 18;M14 12h5M17 10l2 2-2 2`],
  ['mirrorRightToLeft', `${MIRROR_BOX};dt:M12 3v18;nM:r12 3 9 18;M10 12H5M7 10l-2 2 2 2`],
  ['mirrorTopToBottom', `${MIRROR_BOX};dt:M3 12h18;nM:r3 3 18 9;M12 14v5M10 17l2 2 2-2`],
  ['mirrorBottomToTop', `${MIRROR_BOX};dt:M3 12h18;nM:r3 12 18 9;M12 10V5M10 7l2-2 2 2`],
  ['mirrorTopLeft', `${MIRROR_BOX};dt:M12 3v18M3 12h18;nM:r3 3 9 9`],
  ['mirrorTopRight', `${MIRROR_BOX};dt:M12 3v18M3 12h18;nM:r12 3 9 9`],
  ['mirrorBottomLeft', `${MIRROR_BOX};dt:M12 3v18M3 12h18;nM:r3 12 9 9`],
  ['mirrorBottomRight', `${MIRROR_BOX};dt:M12 3v18M3 12h18;nM:r12 12 9 9`],
  // Two Rail Sweep and Extrude and Weave: cross sections at every rail
  // node.
  [
    'sectionAllNodes',
    'tV:M3 6h18M3 18h18;M:M6 6v12M12 6v12M18 6v12;f:c6 6 1.3;f:c12 6 1.3;f:c18 6 1.3',
  ],
]);

/** Menu commands whose icon is not the icon of a Design tab tool running the same command. */
export const COMMAND_ICONS: Readonly<Partial<Record<AppCommand, string>>> = {
  // File.
  'file.newFromTemplate': 'fileNewTemplate',
  'file.saveAs': 'fileSaveAs',
  'file.saveAsTemplate': 'fileNewTemplate',
  'file.incrementSave': 'fileSave',
  'file.print': 'print',
  'file.printPreview': 'printPreview',
  'file.importModel': 'importModel',
  'file.importToolpaths': 'importToolpaths',
  'file.exportEps': 'exportVectors',
  'file.exportDxf': 'exportVectors',
  'file.exportAi': 'exportVectors',
  'file.exportSvg': 'exportVectors',
  'file.exportPdf': 'exportVectors',
  'file.openDataFolder': 'folder',
  'file.clearRecent': 'clearRecent',
  'file.close': 'fileClose',
  'file.printSetup': 'printSetup',
  'file.saveViewImage': 'saveImage',
  // Edit.
  'edit.cut': 'editCut',
  'edit.copy': 'editCopy',
  'edit.paste': 'editPaste',
  'edit.delete': 'editDelete',
  'edit.duplicate': 'editCopy',
  'edit.selectAll': 'selectAll',
  'select.vectorSelector': 'vectorSelection',
  'select.openVectors': 'selectOpen',
  'select.duplicates': 'selectDuplicates',
  'select.currentLayer': 'selectLayer',
  'select.insideBoundary': 'selectInside',
  'edit.selectNone': 'selectNone',
  'edit.xor': 'exclusiveOr',
  'edit.swapSides': 'swapSides',
  'text.adjust': 'adjustText',
  'text.removeFromCurve': 'textOffCurve',
  'tool.nodeToggle': 'nodeEditing',
  'bitmap.editPicture': 'editPicture',
  'form.align': 'align',
  'edit.joinVectors': 'join',
  'edit.curveFit': 'fitCurves',
  'edit.nest': 'nest',
  'edit.jobSetup': 'jobSetup',
  'edit.notes': 'notes',
  'edit.documentVariables': 'variables',
  'view.snapSettings': 'snapSettings',
  'app.preferences': 'options',
  'gadgets.jointCreator': 'jointCreator',
  // Model: the Modeling Tools the items open.
  'model.twoRailSweep': 'twoRailSweep',
  'model.extrudeWeave': 'extrudeWeave',
  'model.turnSpin': 'turnSpin',
  'model.texturedArea': 'textureArea',
  'model.addDraft': 'addDraft',
  'model.offsetModel': 'offsetModel',
  'model.slice': 'sliceModel',
  'model.importModel': 'importModel',
  'model.exportStl': 'exportModel',
  'model.exportGrayscale': 'exportModel',
  'model.exportClipart': 'exportModel',
  'model.sculpt': 'sculpt',
  'model.smooth': 'smoothComponents',
  'model.emboss': 'embossComponent',
  'model.scaleZ': 'scaleModelHeight',
  'model.distort': 'distort',
  'model.replaceBelow': 'replaceBelow',
  'model.properties': 'properties',
  'model.addZeroPlane': 'zeroPlane',
  'model.fromPreview': 'fromPreview',
  // Machine and Toolpaths.
  'machine.configuration': 'machine',
  'machine.add': 'machine',
  'machine.postManagement': 'post',
  'machine.installPost': 'post',
  'machine.installOfflinePackage': 'machinePackage',
  'machine.searchOnline': 'searchOnline',
  'machine.updatePostDatabase': 'postDatabase',
  'toolpaths.materials': 'materialSetup',
  'toolpaths.showTab': 'toolpathsTab',
  'toolpaths.draw2dPreviews': 'preview2d',
  'toolpaths.solidPreviews': 'previewSolid',
  'toolpaths.autoOpen3d': 'autoOpen3d',
  'toolpaths.drawRapids': 'moveRapid',
  'toolpaths.drawPlunges': 'movePlunge',
  'toolpaths.drawRetracts': 'moveRetract',
  'toolpaths.drawJoins': 'moveJoin',
  'preview.standard': 'quality1',
  'preview.high': 'quality2',
  'preview.veryHigh': 'quality3',
  'preview.extremelyHigh': 'quality4',
  'preview.maximum': 'quality5',
  // View, Help and Gadgets.
  'view.zoomToJob': 'zoomJob',
  'view.zoomToSelection': 'zoomSelection',
  'view.saveShadedImage': 'saveImage',
  'view.guidesVisible': 'guidesVisible',
  'view.clearGuides': 'guidesDelete',
  'view.lockGuides': 'guidesLock',
  'view.unlockGuides': 'guidesUnlock',
  'view.drawOrigin': 'origin',
  'view.drawModelingPlane': 'modelingPlane',
  'view3d.showBlock': 'materialBlock',
  'view.multiSided': 'multiSided',
  'view.autoWrap': 'autoWrap',
  'view.colorShaded': 'colorShaded',
  'view.shadedBackground': 'shadedBackground',
  'view.lightFollows': 'lightFollows',
  'view.shadowShading': 'shadowShading',
  'view.tileHorizontal': 'tileHorizontal',
  'view.tileVertical': 'tileVertical',
  'view.cascade': 'cascade',
  'view.newFurniture': 'furniture',
  'help.contents': 'help',
  'help.shortcuts': 'keyboard',
  'help.videoTutorials': 'video',
  'help.whatsNew': 'whatsNew',
  'help.releaseNotes': 'releaseNotes',
  'help.licences': 'licenses',
  'help.enterLicense': 'licenseKey',
  'help.onlineFaq': 'faq',
  'help.userForum': 'forum',
  'help.supportOnline': 'support',
  'help.userPortal': 'userPortal',
  'help.postEditingGuide': 'postGuide',
  'help.migrate': 'migrate',
  'help.checkUpdates': 'checkUpdates',
  'help.kickstarter': 'kickstarter',
  'help.about': 'about',
  // Gadgets.
  'gadgets.install': 'gadgetInstall',
  'gadgets.shortcuts': 'keyboard',
};
