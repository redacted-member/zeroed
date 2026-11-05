// The cutting tool the animated preview draws: the tool's outline turned
// round its axis (a cone for a V-bit, a flat or round ended cylinder for an
// end mill or ball nose) with a narrower shank above it, placed with its
// tip where the tool is (job x -> x, job y -> -z, job z -> y with the
// material top at y zero). A tool whose flute length is set shows its shank
// at the size the collision checks use, and the holder when it is checked.

/** How much taller than the cutting part the shank stands, as a fraction. */
const SHANK_HEIGHT = 0.6;
/** The shank's radius as a fraction of the cutter's widest radius. */
const SHANK_RADIUS = 0.8;
/** Shortest shank, mm, so a tiny tool still shows one. */
const SHANK_MIN = 3;

/** A point of the turned profile: radius and height above the tip, mm. */
export type ProfilePoint = readonly [number, number];

/** A cylinder of the tool: radius, and its bottom and top above the tip, mm. */
export type ToolPart = readonly [number, number, number];

export interface ToolShape {
  /** The cutting part, tip first, closed at the axis at both ends. */
  cutter: ProfilePoint[];
  /** The shank above it, closed at the axis at both ends. */
  shank: ProfilePoint[];
  /** The holder above the shank, closed at the axis, or null. */
  holder: ProfilePoint[] | null;
  /** Widest radius and full height, mm. */
  radius: number;
  height: number;
}

function cylinder(radius: number, bottom: number, top: number): ProfilePoint[] {
  return [
    [0, bottom],
    [radius, bottom],
    [radius, top],
    [0, top],
  ];
}

/**
 * The turned profiles of a tool whose outline is `outline` ([radius,
 * height above the tip] pairs, tip first), with the `shank` and `holder`
 * the checks use when given, or null when it has no size.
 */
export function toolShape(
  outline: ReadonlyArray<readonly [number, number]>,
  parts: { shank?: ToolPart | null; holder?: ToolPart | null } = {},
): ToolShape | null {
  const points = outline.filter(([r, h]) => Number.isFinite(r) && Number.isFinite(h) && r >= 0);
  const last = points[points.length - 1];
  if (last === undefined) {
    return null;
  }
  const radius = Math.max(...points.map(([r]) => r));
  const top = last[1];
  if (radius <= 1e-6 || top <= 1e-6) {
    return null;
  }
  const cutter: ProfilePoint[] = [];
  const first = points[0];
  // A flat or rounded tip is closed across the axis.
  if (first !== undefined && first[0] > 1e-6) {
    cutter.push([0, first[1]]);
  }
  for (const [r, h] of points) {
    const previous = cutter[cutter.length - 1];
    if (previous === undefined || previous[0] !== r || previous[1] !== h) {
      cutter.push([r, h]);
    }
  }
  cutter.push([0, top]);
  const given = parts.shank ?? null;
  const shank =
    given !== null && given[0] > 0 && given[2] > top
      ? cylinder(given[0], top, given[2])
      : cylinder(
          Math.max(last[0], radius) * SHANK_RADIUS,
          top,
          top + Math.max(top * SHANK_HEIGHT, SHANK_MIN),
        );
  const shankTop = shank[2]?.[1] ?? top;
  const held = parts.holder ?? null;
  const holder =
    held !== null && held[0] > 0 && held[2] > held[1]
      ? cylinder(held[0], Math.max(held[1], 0), held[2])
      : null;
  const height = Math.max(shankTop, holder?.[2]?.[1] ?? 0);
  return { cutter, shank, holder, radius, height };
}

/** The tool on a rotary job shown wrapped: where its tip is in the scene, and
 * how far the blank has turned to bring that spot under it, in radians. */
export interface ToolPose {
  at: [number, number, number];
  turn: number;
}

/**
 * The tool at job point `tip` on a rotary job shown wrapped. As on the machine
 * it stays upright over the top of the blank, moving along the axis and in
 * height only, and the blank turns the spot it is cutting up to meet it.
 */
export function wrappedToolPose(
  tip: { x: number; y: number; z: number },
  rotary: { axis: 'x' | 'y'; diameter: number },
  surfaceZ: number,
): ToolPose {
  const radius = rotary.diameter / 2;
  const alongX = rotary.axis === 'x';
  const [along, around] = alongX ? [tip.x, tip.y] : [tip.y, tip.x];
  const height = tip.z - surfaceZ;
  return {
    at: alongX ? [along, height, 0] : [0, height, -along],
    turn: radius > 0 ? around / radius : 0,
  };
}

/** Scene position of the tool tip at job point `tip` over a material top at `surfaceZ`. */
export function toolScenePosition(
  tip: { x: number; y: number; z: number },
  surfaceZ: number,
): [number, number, number] {
  return [tip.x, tip.z - surfaceZ, -tip.y];
}
