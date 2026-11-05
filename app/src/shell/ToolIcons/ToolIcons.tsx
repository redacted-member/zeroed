// Line icons for the left panel tool grids, the menus and the form
// buttons, drawn on a 24 by 24 grid in the neutral text color with one or
// two parts in a role color. Vector tools show the shape, transforms show
// arrows, booleans show two overlapping circles with the kept region
// filled, modeling tools show a relief profile on a gold surface and
// toolpath tools show a cutter or cut motif in wood.
//
// Each icon is a compact spec, drawn on demand, so the start bundle stays
// small: shapes separated by ';', each an optional flag set and ':'
// (iconSpec.ts: t thin, d dash, f fill, n tint, s and w surfaces, upper
// case letters color roles), then either path data (starting with M) or a
// letter and numbers: r x y width height [corner radius], c cx cy r, or
// e cx cy rx ry.

import { useEffect, useSyncExternalStore, type JSX } from 'react';

import { flagClasses, iconShapes, type IconShape } from './iconSpec';

import './ToolIcons.scss';

function shape({ flags, body }: IconShape, key: number): JSX.Element {
  const className = flagClasses(flags);
  if (body.startsWith('M') || body.startsWith('m')) {
    return <path className={className} d={body} key={key} />;
  }
  const [a, b, c, d, e] = body.slice(1).trim().split(/\s+/);
  switch (body[0]) {
    case 'r':
      return <rect className={className} height={d} key={key} rx={e} width={c} x={a} y={b} />;
    case 'c':
      return <circle className={className} cx={a} cy={b} key={key} r={c} />;
    default:
      return <ellipse className={className} cx={a} cy={b} key={key} rx={c} ry={d} />;
  }
}

/** Draws an icon spec (see the top of this file). */
export function drawIcon(spec: string): JSX.Element {
  return (
    <svg aria-hidden="true" className="tool-icon" focusable="false" viewBox="0 0 24 24">
      {iconShapes(spec).map(shape)}
    </svg>
  );
}

// Two circles of radius 6 centred at (9, 12) and (15, 12) cross at
// (12, 6.8) and (12, 17.2).
const BOOLEAN_OUTLINES = 't:c9 12 6;t:c15 12 6';

// Align Objects parts: the material sheet with a folded corner, the frame of
// the object aligned to, and centering arrows across and up and down.
export const SHEET = 't:M3 3h13l5 5v13H3Z';
const FRAME = 't:r3 3 18 18';
const ARROWS_X = 't:M4.5 13H9M15 13h4.5M6 11.5 4.5 13 6 14.5M18 11.5l1.5 1.5-1.5 1.5';
const ARROWS_Y = 't:M12 4.5V10M12 16v4.5M10.5 6 12 4.5 13.5 6M10.5 19 12 20.5l1.5-1.5';

// A relief profile standing on a base line.
const BASE = 'M2 20h20';

const ICONS = new Map<string, string>([
  ['fileOpen', 'M3 6h6l2 2h8v3;s:M3 6v13h15l3-8H7l-4 8'],
  ['importVectors', 'tV:M4 20c3-8 6-10 10-10;f:c4 20 1.5;f:c14 10 1.5;M17 3v8M14 8l3 3 3-3'],
  ['exportModel', 'M2 20h20;Ms:M3 20c2-6 5-9 9-9s7 3 9 9;M12 3v7M9 7l3 3 3-3'],
  // File and view.
  ['jobSetup', 'r3 5 18 14 1;tR:M3 15h4v4;fR:c3 19 1.75'],
  // Create vectors.
  ['drawPolyline', 'V:M3 18 9 7l6 9 6-11;f:c3 18 1.5;f:c21 5 1.5'],
  ['drawArc', 'V:M4 17a8 8 0 0 1 16 0;f:c4 17 1.5;f:c20 17 1.5'],
  ['drawCircle', 'V:c12 12 8;f:c12 12 1.25'],
  ['drawEllipse', 'V:e12 12 9 5.5'],
  // Two boards meeting at a corner, their fingers interlocked.
  ['jointCreator', 'M4 3h6v18H4Z;M10 14h10v7H10Z;nV:M4 14h3v3.5H4Z;nV:M7 17.5h3V21H7Z'],
  ['drawRectangle', 'V:r4 6 16 12'],
  ['drawPolygon', 'V:M12 3l7.8 4.5v9L12 21l-7.8-4.5v-9Z'],
  [
    'drawStar',
    'V:M12 3l2.35 6.26 6.69.3-5.24 4.18 1.78 6.45L12 16.5l-5.58 3.69 1.78-6.45-5.24-4.18 6.69-.3Z',
  ],
  ['createText', 'V:M5 7V4h14v3M12 4v16M9 20h6'],
  ['textOnCurve', 'M7 4h10M12 4v9;tV:M3 18c3-3 6-3 9 0s6 3 9 0'],
  ['dimensions', 't:M4 8v11M20 8v11;V:M4 14h16M7 12l-3 2 3 2M17 12l3 2-3 2'],
  [
    'traceBitmap',
    't:r3 3 18 18;t:M9 3v18M15 3v18M3 9h18M3 15h18;V:M7 17c-2-4 1-10 5-10s7 5 5 9-8 5-10 1Z',
  ],
  // Transform.
  ['move', 'V:M12 3v18M3 12h18M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3'],
  ['setSize', 'V:r4 10 10 10;M14 10l6-6M15 4h5v5'],
  ['rotate', 'V:M20 12a8 8 0 1 1-2.34-5.66M13.7 6.3h4v-4'],
  ['mirror', 'dt:M12 2v20;nV:M10 6 3 18h7Z;R:M14 6l7 12h-7Z'],
  ['distort', 'dt:M4 7h16v12H4Z;V:M4 19V9q8-7 16 0v10Z;fS:c12 3.75 1.75'],
  [
    'vectorUnwrapper',
    't:e6 12 2.5 7;t:M6 5h5M6 19h5;nV:M13 5h8v14h-8Z;V:M13 5h8v14h-8ZM11 12h-3M10 10l2 2-2 2',
  ],
  ['align', 'M4 3v18;V:r7 5 12 5;V:r7 14 7 5'],
  // Align Objects: a small box moved inside or against a larger one, or
  // centered on the material sheet (folded corner).
  ['alignMaterial', `${SHEET};V:r9 10 6 6;${ARROWS_X};${ARROWS_Y}`],
  ['alignMaterialX', `${SHEET};V:r9 10 6 6;${ARROWS_X}`],
  ['alignMaterialY', `${SHEET};V:r9 10 6 6;${ARROWS_Y}`],
  ['alignLeft', `${FRAME};M3 3v18;nV:r5 8 6 8;t:M19 12h-5.5M15 10.5 13.5 12l1.5 1.5`],
  ['alignRight', `${FRAME};M21 3v18;nV:r13 8 6 8;t:M5 12h5.5M9 10.5l1.5 1.5L9 13.5`],
  ['alignTop', `${FRAME};M3 3h18;nV:r8 5 8 6;t:M12 19v-5.5M10.5 15l1.5-1.5 1.5 1.5`],
  ['alignBottom', `${FRAME};M3 21h18;nV:r8 13 8 6;t:M12 5v5.5M10.5 9l1.5 1.5L13.5 9`],
  ['alignCenter', `${FRAME};V:r9 10 6 6;${ARROWS_X};${ARROWS_Y}`],
  ['alignHCenter', `${FRAME};V:r9 10 6 6;${ARROWS_X}`],
  ['alignVCenter', `${FRAME};V:r9 10 6 6;${ARROWS_Y}`],
  ['alignOutsideLeft', 'r13 4 8 16;V:r3 9 6 6;M9 12h3M10.5 10.5 12 12l-1.5 1.5'],
  ['alignOutsideRight', 'r3 4 8 16;V:r15 9 6 6;M15 12h-3M13.5 10.5 12 12l1.5 1.5'],
  ['alignOutsideTop', 'r4 13 16 8;V:r9 3 6 6;M12 9v3M10.5 10.5 12 12l1.5-1.5'],
  ['alignOutsideBottom', 'r4 3 16 8;V:r9 15 6 6;M12 15v-3M10.5 13.5 12 12l1.5 1.5'],
  // The extra File Operations row.
  ['fileNew', 'M6 3h8l4 4v14H6ZM14 3v4h4'],
  ['fileSave', 'M4 4h13l3 3v13H4Z;V:M8 4v5h7V4M7 20v-6h10v6'],
  ['editUndo', 't:M8 3h7l4 4v14H8;V:M16 15H7a3 3 0 0 1 0-6h1M5 13l-2 2 2 2'],
  ['editRedo', 't:M5 3h7l4 4v4M5 3v18h11;V:M8 15h9a3 3 0 0 0 0-6h-1M19 13l2 2-2 2'],
  ['group', 'dt:r2.5 2.5 19 19;V:r5 5 6 6;V:c16 16 3.25'],
  [
    'textToCurves',
    'M4 18L8.5 6L13 18M5.7 14h5.6;tV:M14.5 17c1.5-6 5-6 6.5-1;S:c14.5 17 1.1;S:c21 16 1.1',
  ],
  ['ungroup', 'dt:r2 2 10 10;dt:r12 12 10 10;V:r4.5 4.5 5 5;V:c17 17 2.75'],
  // Edit, booleans first.
  ['vectorSelection', 'M6 3v15l4-4 3 7 2-1-3-7h6z'],
  ['nodeEditing', 'tV:M4 18C7 6 15 6 20 16;S:M3 17h3v3H3zM10.5 7.5h3v3h-3zM18.5 14.5h3v3h-3z'],
  [
    'interactiveSelection',
    'dS:M4 4h16v16H4z;M2.5 2.5h3v3h-3zM18.5 2.5h3v3h-3zM2.5 18.5h3v3h-3zM18.5 18.5h3v3h-3z;t:M9 12h6M12 9v6',
  ],
  ['weld', 'nV:M12 6.8A6 6 0 1 0 12 17.2A6 6 0 1 0 12 6.8Z'],
  ['subtract', `${BOOLEAN_OUTLINES};nV:M12 6.8A6 6 0 1 0 12 17.2A6 6 0 0 1 12 6.8Z`],
  ['intersect', `${BOOLEAN_OUTLINES};nV:M12 6.8A6 6 0 0 0 12 17.2A6 6 0 0 0 12 6.8Z`],
  ['trimObjects', 'd:r8 8 12 12;R:M3 13h5M13 3v5;nV:M8 8h12v12H8z'],
  ['join', 'V:M3 18 9 9M15 9l6 9;dR:M9 9h6;f:c9 9 1.5;f:c15 9 1.5'],
  ['close', 'V:M12 4l8 15H4;dR:M4 19 12 4'],
  ['closeSmooth', 'V:M12 4l8 15H4;dR:M4 19C2 12 6 6 12 4'],
  ['closeMoveEnds', 'V:M4 19 12 7l8 12;fS:c12 7 1.8'],
  ['trim', 'V:M3 12h18M12 3v9;dtR:M12 12v9'],
  ['drawFreehand', 'V:M3 18c2-7 5-11 7-7s1 7 4 5 3-9 7-11;f:c3 18 1.5'],
  ['measure', 'M3 16 16 3l5 5L8 21Z;tV:m7 12 2 2m1-5 2 2m1-5 2 2'],
  [
    'vectorTexture',
    't:r3 3 18 18 2;V:M5 8c2-2 4 2 7 0s5-2 7 0M5 13c2-2 4 2 7 0s5-2 7 0M5 18c2-2 4 2 7 0s5-2 7 0',
  ],
  ['editPicture', 'r3 5 18 14;tG:m3 16 5-5 4 4 3-3 6 6;tA:c16 9 1.5'],
  ['cropBitmap', 'M6 2v16h16;M2 6h16v16;tG:m9 15 3-3 2 2 4-4'],
  ['extend', 't:M20 4v16;V:M4 12h8;dR:M12 12h8;f:c4 12 1.5'],
  ['fillet', 'dt:M5 13V5h8;V:M5 20v-7M13 5h7;R:M5 13a8 8 0 0 1 8-8'],
  ['fitCurves', 'dt:M3 16 9 8l6 8 6-8;V:M3 16c2-5 4-8 6-8s4 8 6 8 4-3 6-8'],
  ['validator', 'V:c12 12 8.5;G:M8 12.5l3 3 5-6'],
  ['boundary', 'dtR:r3 4 18 16 4;c9 10 2.75;r12 11 5 5'],
  // Offset and layout.
  ['offset', 'V:r7 7 10 10;tR:r3 3 18 18 4'],
  ['arrayCopy', 'V:r4 4 6 6;r14 4 6 6;r4 14 6 6;r14 14 6 6'],
  ['nest', 'r3 3 18 18;V:M6 6h6v5H6z;V:M14 6h4v8h-4z;V:M6 14l5 0-2.5 4z'],
  ['plates', 'r3 3 18 18;V:r5 5 7 5;V:r13 5 7 5;V:r5 13 7 5;t:M14 15.5h5'],
  [
    'circularArray',
    'V:c19 12 1.9;V:c15.5 18.06 1.9;V:c8.5 18.06 1.9;V:c5 12 1.9;V:c8.5 5.94 1.9;V:c15.5 5.94 1.9;f:c12 12 1',
  ],
  ['copyAlong', 'dt:M3 19C8 19 16 5 21 5;V:r3.4 16.1 4 4;V:r10 10 4 4;V:r16.6 3.9 4 4'],
  // Modeling.
  ['createFlatShape', `${BASE};Ms:M4 20V13H20V20`],
  ['createShape', `${BASE};Ms:M4 20C4 9 20 9 20 20`],
  ['createAngledShape', `${BASE};Ms:M4 20L12 10L20 20`],
  ['createSmoothShape', `${BASE};Ms:M3 20C8 20 8 10 12 10S16 20 21 20`],
  ['createCustomShape', `${BASE};Ms:M4 20V15L9 12L13 14L20 9V20;t:M15 4h5v4`],
  ['importModel', 'M:M3 14v6h18v-6;M12 3v11M8 10l4 4 4-4'],
  ['twoRailSweep', 'tV:M3 7c5-3 13-3 18 0M3 19c5-3 13-3 18 0;M:M12 4.75q-6 6 0 12'],
  ['turnSpin', 'dt:M12 2v20;M:M12 4c4 1 6 4 5 8s-4 5 1 8h-6;dt:M12 4c-4 1-6 4-5 8s4 5-1 8h6'],
  ['extrudeWeave', 'V:M3 8c6 0 12 8 18 8;M:M3 16c6 0 7-3 8-4M13 10c2-1.5 4-2 8-2'],
  ['textureArea', 't:r3 4 18 16 1;M:M6 9q2-2 4 0t4 0 4 0M6 15q2-2 4 0t4 0 4 0'],
  ['bitmapRelief', `${BASE};t:r3 3 18 8;nG:M3 5h6v6H3Z;Ms:M3 20v-4c4 0 4-3 9-3s5 3 9 3v4`],
  ['componentBoundary', 'dtV:r3 4 18 16;M5 16h14;Ms:M6 16c2-7 10-7 12 0'],
  ['bakeComponents', 'M2 20h20;Ms:M4 20c1-8 7-8 8 0c1-8 7-8 8 0;M7 3v4l5 3 5-3V3M12 10v3'],
  [
    'fromVisible',
    'M2 20h20;t:M3 20c1-6 5-6 6 0M9 20c1-8 6-8 7 0;Ms:M14 20c1-5 6-5 7 0;M14 20c1-5 6-5 7 0M12 3a8 5 0 0 1 8 5M17 8h3V5',
  ],
  [
    'crossSection',
    'Ms:M4 20c1-11 15-11 16 0z;M2 20h20;V:M2 13h3M8 13h3M14 13h3M20 13h2;M3 9v8M21 9v8',
  ],
  ['sliceModel', 'Ms:M4 20c1-13 15-13 16 0z;M2 20h20M3 15h18M5 10h14'],
  ['clearInside', 'Ms:M3 20c1-11 17-11 18 0h-5v-5H8v5z;M2 20h20;R:M8 20v-5h8v5'],
  ['clearOutside', 'Ms:M8 20v-7h8v7z;M2 20h20M8 20v-7h8v7;dtR:M3 20c1-11 17-11 18 0'],
  ['splitComponent', 'Ms:M3 20c1-11 17-11 18 0z;M2 20h20;R:M12 3v19'],
  [
    'replaceBelow',
    'M2 20h20;Ms:M7 11c1-7 9-7 10 0z;M3 11h2M8 11h2M14 11h2M19 11h2M6 20c0-3 1-6 2-9M18 20c0-3-1-6-2-9',
  ],
  ['sculpt', 'Ms:M2 20v-2c3-4 5-6 8-3s6 2 12-3v8Z;dtS:c15 7 4;f:c15 7 1'],
  ['smoothComponents', `${BASE};Ms:M3 20c3-9 15-9 18 0z;t:M5 8c2-2 4 2 6 0s4 2 6 0 3 1 3 1`],
  [
    'embossComponent',
    `${BASE};td:M4 20c1-12 15-12 16 0;Ms:M4 20c1-4 3-5 4-3s2-2 4-2 2 3 4 1 3 1 4 4z`,
  ],
  [
    'scaleModelHeight',
    `${BASE};Ms:M3 20c2-7 11-7 13 0z;M20 4v13M17.5 6.5 20 4l2.5 2.5M17.5 14.5 20 17l2.5-2.5`,
  ],
  ['addDraft', `${BASE};Ms:M3 20 8 8h8l5 12z;td:M8 8v12M16 8v12`],
  ['offsetModel', `${BASE};Ms:M6 20c1-9 11-9 12 0z;tR:M3 20c1-13 17-13 18 0`],
  // Toolpath operations.
  // Toolpath management.
  ['editToolpath', 'tP:M3 18c4-9 8-9 12 0;M14 4l6 6-8 8H6v-6Z'],
  ['duplicateToolpath', 'r3 3 12 12;nP:r9 9 12 12;P:M9 9h12v12H9Z'],
  ['deleteToolpath', 'R:M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15;tR:M10 10v7M14 10v7'],
  ['recalculateAll', 'G:M20 12a8 8 0 1 1-2.34-5.66;G:M20 3v5h-5;t:M8 14c2-4 6-4 8 0'],
  ['loadTemplate', 'M6 3h8l4 4v14H6z;tP:M9 14h6M12 11v7M9 15l3 3 3-3'],
  ['saveTemplate', 'M6 3h8l4 4v14H6z;tP:M12 18v-7M9 14l3-3 3 3'],
  ['saveVisibleTemplate', 'M8 3h8l4 4v14H8z;t:M4 7v16h12;tP:M14 17v-6M11.5 13.5 14 11l2.5 2.5'],
  ['mergeToolpaths', 'tP:M3 5h6c3 0 3 7 6 7h6M3 19h6c3 0 3-7 6-7;M18 9l3 3-3 3'],
  ['toolpathSummary', 'c12 12 8.5;R:M12 7v5l3.5 2'],
  ['keepOutZones', 'dt:r4 4 16 16;R:c12 12 5;R:M8.5 15.5l7-7'],
  ['jobSheet', 'M5 3h14v18H5z;tP:M8 8h8M8 12h8M8 16h5'],
  ['saveToolpaths', 'M6 3h8l4 4v14H6ZM14 3v4h4;P:M12 9v8M9 14l3 3 3-3'],
  ['toolDatabase', 'r9 2 6 7;M9 9h6v10l-3 3-3-3Z;t:M9 12l6 3M9 16l6 3'],
  ['toolpathTiling', 'r3 5 18 14;nP:M9 5h3v14H9z;t:M9 5v14M12 5v14'],
  ['preview', 'r3 5 18 14 1;fR:M10 9v6l5-3Z'],
  ['assemblyView', 'w:M4 20V10l4-4h8v14Z;w:M16 6l4-3v14l-4 3Z;t:M8 6v14M4 10h12'],
  // 2D toolpaths.
  ['profile', 'Vw:r7 7 10 10;dtP:r3.5 3.5 17 17 3.5;f:c20.5 12 2'],
  ['pocket', 'Vw:r4 4 16 16;tP:M7 7h10v2.5H7V12h10v2.5H7V17h10'],
  ['drill', 'M10 2v12l2 3 2-3V2;t:M10 6l4 2M10 10l4 2;P:M3 18h6v4h6v-4h6'],
  ['vgroove', 'M9 2v5l3 5 3-5V2;nP:M3 14h5l4 7 4-7h5'],
  ['quickEngrave', 'M15 4v6l3 5 3-5V4;tP:M3 19c3-5 5 1 8-3s5-2 7-1'],
  ['arrayCopyToolpath', 'P:r3 14 6 6;dt:r15 14 6 6;dt:r3 4 6 6;dt:r15 4 6 6'],
  ['chamfer', 'w:M3 20V8h11l7 7v5;dtP:M14 8h7v7'],
  ['photoCarve', 'w:r3 4 18 16 1;t:M6 8h12M6 12h12M6 16h12;P:M6 8h3M6 12h6M6 16h9'],
  ['straightInlay', 'M3 10h6v8h6v-8h6;w:M10 3h4v6h-4Z;t:M12 9v4'],
  ['drillBank', 'M3 5h18v4H3Z;M6 9v7M10 9v7M14 9v7M18 9v7;tP:M5 20h14'],
  ['threadMilling', 'M9 3v4M15 3v4;P:M7 7h10v2l-10 2v2l10 2v2l-10 2v2h10'],
  ['fluting', 'nP:M3 8c5-3 13-3 18 0c-5 3-13 3-18 0Z;nP:M3 16c5-3 13-3 18 0c-5 3-13 3-18 0Z'],
  ['prismCarve', 'w:r3 6 18 12;t:M3 6l5 6-5 6M21 6l-5 6 5 6;P:M8 12h8'],
  [
    'texture',
    'P:M3 5l5 1M11 4l5 1.5M19 5l2 .5;P:M4 10l5 .5M12 9.5l5 1;P:M3 15l4 1M10 14.5l5 .5M18 15l3 1;P:M5 20l5-.5M13 19.5l5 .5',
  ],
  ['inlay', 'w:M7 3h10l-5 8Z;P:M2 20h6l4-6 4 6h6'],
  ['moulding', 'w:M3 18V5h4c0 4 3 5 6 7s4 3 8 3v3Z;dtP:M3 21h18'],
  // 3D toolpaths.
  ['rough3d', `${BASE};P:M3 20v-4h3v-4h3V8h6v4h3v4h3v4;dtM:M4 20C5 7 19 7 20 20`],
  ['finish3d', `${BASE};w:M3 20C4 7 20 7 21 20;M10 1v5a2 2 0 0 0 4 0V1`],
  ['laserCutFill', 'M8 2h8v5H8z;tR:M12 7v9;M3 20h6l3-4 3 4h6'],
  ['laserPicture', 'w:M3 4h18v16H3z;t:M6 8h12M6 12h9M6 16h5;nR:M14 12l4 4h-7z'],
  ['sketchCarve', 'tP:M4 18c3-9 6-12 9-11s2 6-2 9 4 1 7-5;M15 3l3 3-9 9H6v-3z'],
  ['laserSketch', 'M8 2h8v4H8z;tR:M12 6v5;tP:M3 21c3-6 6-8 9-7s3 4 0 5 5 1 9-5'],
]);

// The icons only menus, form buttons and panel row buttons draw live in
// extraIcons.tsx, loaded after start up so the start bundle stays small.
let extra: ReadonlyMap<string, string> | null = null;
let commands: ReadonlyMap<string, string> = new Map();
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();

/** Loads the menu, form and panel icons (extraIcons.tsx) once; later calls share the load. */
export function loadExtraIcons(): Promise<void> {
  loading ??= import('./extraIcons').then(
    (module) => {
      extra = module.EXTRA_ICONS;
      commands = new Map(
        Object.entries(module.COMMAND_ICONS).filter(
          (entry): entry is [string, string] => entry[1] !== undefined,
        ),
      );
      for (const listener of listeners) {
        listener();
      }
    },
    (error: unknown) => {
      loading = null;
      console.error('the menu icons could not be loaded', error);
    },
  );
  return loading;
}

function isLoaded(): boolean {
  return extra !== null;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Starts loading the menu, form and panel icons and renders the caller
 * again once they are in. Answers whether they are.
 */
export function useExtraIcons(): boolean {
  const loaded = useSyncExternalStore(subscribe, isLoaded, isLoaded);
  useEffect(() => {
    void loadExtraIcons();
  }, []);
  return loaded;
}

/** The icon id a menu command names for itself (extraIcons.ts), once the lazy set is in. */
export function commandIcon(command: string): string | undefined {
  return commands.get(command);
}

/** The spec of a tool or menu command icon, or undefined when it has none (or its set is not loaded yet). */
export function iconSpec(id: string): string | undefined {
  return ICONS.get(id) ?? extra?.get(id);
}

/** The icon for a tool or menu command, or null when it has none (or its set is not loaded yet). */
export function toolIcon(id: string): JSX.Element | null {
  const spec = iconSpec(id);
  return spec === undefined ? null : drawIcon(spec);
}
