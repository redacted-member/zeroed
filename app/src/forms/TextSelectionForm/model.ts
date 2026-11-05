// Pure rules for the Text Selection tool: which gap a click lands in, how a
// click changes the letter or line spacing, and the arch handles.

import type { Bbox, GlyphBox, Point, TextArc, TextSpec } from '../../bridge/document';

/** Each click moves letters or lines by this fraction of the text height. */
export const GAP_STEP = 0.02;

export type Gap = { kind: 'letter'; line: number; index: number } | { kind: 'line'; line: number };

export interface ClickKeys {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
}

function lineBand(glyphs: readonly GlyphBox[], line: number): Bbox | null {
  const on = glyphs.filter((g) => g.line === line);
  if (on.length === 0) {
    return null;
  }
  return [
    Math.min(...on.map((g) => g.bbox[0])),
    Math.min(...on.map((g) => g.bbox[1])),
    Math.max(...on.map((g) => g.bbox[2])),
    Math.max(...on.map((g) => g.bbox[3])),
  ];
}

/**
 * The gap under the pointer: between two letters of a line (the gap after
 * letter `index`), or between a line and the next one. `height` is the text
 * height, which sets how near a gap the pointer must be.
 */
export function gapAt(glyphs: readonly GlyphBox[], at: Point, height: number): Gap | null {
  const lines = Math.max(0, ...glyphs.map((g) => g.line + 1));
  const reach = height * 0.25;
  for (let line = 0; line < lines; line += 1) {
    const band = lineBand(glyphs, line);
    if (band === null || at.y < band[1] - reach || at.y > band[3] + reach) {
      continue;
    }
    const on = glyphs.filter((g) => g.line === line).sort((a, b) => a.index - b.index);
    for (let i = 0; i + 1 < on.length; i += 1) {
      const a = on[i];
      const b = on[i + 1];
      if (a === undefined || b === undefined) {
        continue;
      }
      const middle = (a.bbox[2] + b.bbox[0]) / 2;
      const half = Math.max(reach / 2, (b.bbox[0] - a.bbox[2]) / 2 + reach / 2);
      if (Math.abs(at.x - middle) <= half) {
        return { kind: 'letter', line, index: a.index };
      }
    }
  }
  for (let line = 0; line + 1 < lines; line += 1) {
    const upper = lineBand(glyphs, line);
    const lower = lineBand(glyphs, line + 1);
    if (upper === null || lower === null) {
      continue;
    }
    const left = Math.min(upper[0], lower[0]);
    const right = Math.max(upper[2], lower[2]);
    if (at.x >= left && at.x <= right && at.y < upper[1] + reach && at.y > lower[3] - reach) {
      return { kind: 'line', line };
    }
  }
  return null;
}

/**
 * The spec after a click on a gap: a plain click closes it, Shift opens it,
 * Ctrl doubles the step and Alt changes every letter gap on that line.
 */
export function clickGap(
  spec: TextSpec,
  glyphs: readonly GlyphBox[],
  gap: Gap,
  keys: ClickKeys,
): TextSpec {
  const step = GAP_STEP * (keys.ctrl ? 2 : 1) * (keys.shift ? 1 : -1);
  if (gap.kind === 'line') {
    const lineGaps = [...(spec.lineGaps ?? [])];
    while (lineGaps.length <= gap.line) {
      lineGaps.push(0);
    }
    lineGaps[gap.line] = (lineGaps[gap.line] ?? 0) + step;
    return { ...spec, lineGaps };
  }
  const letterGaps = (spec.letterGaps ?? []).map((line) => [...line]);
  while (letterGaps.length <= gap.line) {
    letterGaps.push([]);
  }
  const row = letterGaps[gap.line] ?? [];
  const count = glyphs.filter((g) => g.line === gap.line).length;
  const targets = keys.alt
    ? Array.from({ length: Math.max(0, count - 1) }, (_, i) => i)
    : [gap.index];
  for (const index of targets) {
    while (row.length <= index) {
      row.push(0);
    }
    row[index] = (row[index] ?? 0) + step;
  }
  letterGaps[gap.line] = row;
  return { ...spec, letterGaps };
}

/** The two arch handles: the middle of the top and bottom of the text. */
export function archHandles(box: Bbox): { top: Point; bottom: Point } {
  const x = (box[0] + box[2]) / 2;
  return { top: { x, y: box[3] }, bottom: { x, y: box[1] } };
}

/** Which green arch handle: the top one arcs the text upwards, the bottom
 * one downwards. */
export type ArchHandle = 'top' | 'bottom';

/**
 * The arch after dragging a handle up (positive) or down by `dy`
 * millimetres. The top handle only arches the text upwards and the bottom
 * one only downwards; dragging back past straight leaves the text straight.
 */
export function archFromDrag(spec: TextSpec, dy: number, handle: ArchHandle): TextSpec {
  const moved = (spec.arch ?? 0) + dy;
  const arch = handle === 'top' ? Math.max(0, moved) : Math.min(0, moved);
  if (Math.abs(arch) < 1e-6) {
    const next = { ...spec };
    delete next.arch;
    return next;
  }
  return { ...spec, arch };
}

/** The handles arched text adds: turn (red), radius (blue) and move (white). */
export interface ArcHandles {
  /** At the two ends of the baseline: drag to turn the text round the centre. */
  rotate: [Point, Point];
  /** At the two ends of the letter tops: drag to change the radius. */
  radius: [Point, Point];
  /** In the middle of the text and at the centre of the arc: drag to move. */
  move: [Point, Point];
}

export type ArcHandle = keyof ArcHandles;

const onArc = (arc: TextArc, angle: number, radius: number): Point => ({
  x: arc.centre.x + radius * Math.cos(angle),
  y: arc.centre.y + radius * Math.sin(angle),
});

export function arcHandles(arc: TextArc, height: number): ArcHandles {
  const top = arc.outward ? arc.radius + height : Math.max(0, arc.radius - height);
  const middle = (arc.start + arc.end) / 2;
  const mid = arc.outward ? arc.radius + height / 2 : arc.radius - height / 2;
  return {
    rotate: [onArc(arc, arc.start, arc.radius), onArc(arc, arc.end, arc.radius)],
    radius: [onArc(arc, arc.start, top), onArc(arc, arc.end, top)],
    move: [onArc(arc, middle, mid), arc.centre],
  };
}

/** Which arc handle lies within `reach` of a point, if any. */
export function arcHandleAt(handles: ArcHandles, at: Point, reach: number): ArcHandle | null {
  for (const kind of ['rotate', 'radius', 'move'] as const) {
    if (handles[kind].some((h) => Math.hypot(h.x - at.x, h.y - at.y) <= reach)) {
      return kind;
    }
  }
  return null;
}

/** Degrees a turn handle drag turns the text round the arc centre; Ctrl snaps to 15. */
export function rotateFromDrag(arc: TextArc, from: Point, to: Point, snap: boolean): number {
  const a = Math.atan2(from.y - arc.centre.y, from.x - arc.centre.x);
  const b = Math.atan2(to.y - arc.centre.y, to.x - arc.centre.x);
  let degrees = ((b - a) * 180) / Math.PI;
  while (degrees > 180) {
    degrees -= 360;
  }
  while (degrees <= -180) {
    degrees += 360;
  }
  return snap ? Math.round(degrees / 15) * 15 : degrees;
}

/**
 * A radius handle drag: the arch that gives the new radius with the arc
 * centre kept where it is, and the text origin that keeps it there.
 * `rotation` is the text's turn in degrees. The arch and the radius are
 * tied as for a chord of the text's width (the kernel's rule).
 */
export function radiusFromDrag(
  spec: TextSpec,
  origin: Point,
  rotation: number,
  arc: TextArc,
  from: Point,
  to: Point,
): { spec: TextSpec; origin: Point } | null {
  const bend = spec.arch ?? 0;
  if (Math.abs(bend) < 1e-9) {
    return null;
  }
  const half = arc.width / 2;
  const grow =
    Math.hypot(to.x - arc.centre.x, to.y - arc.centre.y) -
    Math.hypot(from.x - arc.centre.x, from.y - arc.centre.y);
  // The chord the kernel starts from: tan(angle / 4) = 2 bend / width.
  const angle = 2 * Math.atan((2 * Math.abs(bend)) / arc.width);
  const radius = Math.max(half + 1e-6, arc.radius + grow);
  const sine = Math.min(1, half / radius);
  const next = angle > Math.PI / 2 ? Math.PI - Math.asin(sine) : Math.asin(sine);
  const arch = Math.sign(bend) * half * Math.tan(next / 2);
  // The centre sits (radius - bend) below the baseline when arched up, and
  // (radius - |bend|) above it when arched down; keep it put.
  const before = arc.radius - Math.abs(bend);
  const after = radius - Math.abs(arch);
  const shift = arc.outward ? after - before : before - after;
  const turn = (rotation * Math.PI) / 180;
  const up = { x: -Math.sin(turn), y: Math.cos(turn) };
  return {
    spec: { ...spec, arch },
    origin: { x: origin.x + up.x * shift, y: origin.y + up.y * shift },
  };
}
