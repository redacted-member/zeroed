// Cursors the 2D view tools switch between while hovering: a crosshair
// with a check mark (a click will act), with a cross (a click removes
// something), and closed or open scissors for Interactive Vector Trim.
// Each is a small SVG drawn here, dark strokes on a light halo so it reads
// on any background, with its hot spot in the middle of the crosshair.

/** Outline widths: the light halo under the dark stroke. */
const HALO = 3;
const STROKE = 1.2;

function svgCursor(body: string, hotX: number, hotY: number, fallback: string): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32" ` +
    `fill="none" stroke-linecap="round" stroke-linejoin="round">` +
    `<g stroke="#fff" stroke-width="${HALO}">${body}</g>` +
    `<g stroke="#111" stroke-width="${STROKE}">${body}</g></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") ${hotX} ${hotY}, ${fallback}`;
}

/** A crosshair centred on (8, 8). */
const CROSSHAIR = '<path d="M8 1v5M8 10v5M1 8h5M10 8h5"/>';

/** Crosshair plus a check mark: the corner under it can take a fillet. */
export const CHECK_CURSOR = svgCursor(`${CROSSHAIR}<path d="M17 21l3 3 7-8"/>`, 8, 8, 'crosshair');

/** Crosshair plus a cross: a click takes out what is under it. */
export const REMOVE_CURSOR = svgCursor(
  `${CROSSHAIR}<path d="M18 17l7 7M25 17l-7 7"/>`,
  8,
  8,
  'crosshair',
);

/** Scissors with the blades shut: nothing to trim under the cursor. */
export const SCISSORS_CLOSED = svgCursor(
  '<circle cx="7" cy="24" r="3.5"/><circle cx="17" cy="24" r="3.5"/>' +
    '<path d="M9 21l6-19M15 21L9 2"/>',
  12,
  12,
  'crosshair',
);

/** Scissors with the blades open: a click trims the span under it. */
export const SCISSORS_OPEN = svgCursor(
  '<circle cx="6" cy="25" r="3.5"/><circle cx="18" cy="25" r="3.5"/>' +
    '<path d="M8.5 22.5L20 3M15.5 22.5L4 3"/>',
  12,
  17,
  'crosshair',
);
