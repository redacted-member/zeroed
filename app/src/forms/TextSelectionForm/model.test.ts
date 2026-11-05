import { describe, expect, it } from 'vitest';

import type { GlyphBox, TextArc, TextSpec } from '../../bridge/document';
import {
  arcHandleAt,
  arcHandles,
  archFromDrag,
  archHandles,
  clickGap,
  gapAt,
  GAP_STEP,
  radiusFromDrag,
  rotateFromDrag,
} from './model';

const glyph = (line: number, index: number, x0: number, x1: number): GlyphBox => {
  const y = -line * 15;
  return { line, index, bbox: [x0, y, x1, y + 10] };
};
// Two lines: "AB" and "CD", letters 8 wide with 2 between.
const glyphs = [glyph(0, 0, 0, 8), glyph(0, 1, 10, 18), glyph(1, 0, 0, 8), glyph(1, 1, 10, 18)];
const spec: TextSpec = {
  text: 'AB\nCD',
  fontFamily: 'Arial',
  bold: false,
  italic: false,
  sizeMm: 10,
  alignment: 'left',
  lineSpacing: 1,
  charSpacing: 0,
  kerning: true,
};
const keys = { shift: false, ctrl: false, alt: false };

describe('text selection model', () => {
  it('finds the gap between letters and between lines', () => {
    expect(gapAt(glyphs, { x: 9, y: 5 }, 10)).toEqual({ kind: 'letter', line: 0, index: 0 });
    expect(gapAt(glyphs, { x: 9, y: -10 }, 10)).toEqual({ kind: 'letter', line: 1, index: 0 });
    expect(gapAt(glyphs, { x: 4, y: -2.5 }, 10)).toEqual({ kind: 'line', line: 0 });
    expect(gapAt(glyphs, { x: 40, y: 5 }, 10)).toBeNull();
  });

  it('closes a gap on click, opens it with Shift, doubles with Ctrl, spreads with Alt', () => {
    const closed = clickGap(spec, glyphs, { kind: 'letter', line: 1, index: 0 }, keys);
    expect(closed.letterGaps).toEqual([[], [-GAP_STEP]]);
    const opened = clickGap(
      spec,
      glyphs,
      { kind: 'line', line: 0 },
      { ...keys, shift: true, ctrl: true },
    );
    expect(opened.lineGaps).toEqual([2 * GAP_STEP]);
    const all = clickGap(
      spec,
      glyphs,
      { kind: 'letter', line: 0, index: 0 },
      { ...keys, alt: true },
    );
    expect(all.letterGaps).toEqual([[-GAP_STEP]]);
  });

  it('arches with the handles', () => {
    expect(archHandles([0, 0, 20, 10])).toEqual({ top: { x: 10, y: 10 }, bottom: { x: 10, y: 0 } });
    expect(archFromDrag(spec, 4, 'top').arch).toBe(4);
    expect(archFromDrag({ ...spec, arch: 4 }, -4, 'top').arch).toBeUndefined();
    // The bottom handle arcs downwards.
    expect(archFromDrag(spec, -3, 'bottom').arch).toBe(-3);
  });

  it('arches upwards with the top handle and downwards with the bottom one only', () => {
    // Dragging the top handle down, or the bottom one up, leaves it straight.
    expect(archFromDrag(spec, -5, 'top').arch).toBeUndefined();
    expect(archFromDrag(spec, 5, 'bottom').arch).toBeUndefined();
    expect(archFromDrag({ ...spec, arch: 6 }, -10, 'top').arch).toBeUndefined();
    expect(archFromDrag({ ...spec, arch: -6 }, 2, 'bottom').arch).toBe(-4);
  });
});

describe('arched text handles', () => {
  // Width 40 arched up by 10: the kernel's chord rule gives these numbers.
  const angle = 2 * Math.atan((2 * 10) / 40);
  const radius = 20 / Math.sin(angle);
  const arc: TextArc = {
    centre: { x: 20, y: 10 - radius },
    radius,
    start: Math.PI / 2 + 20 / radius,
    end: Math.PI / 2 - 20 / radius,
    width: 40,
    outward: true,
  };
  const arched: TextSpec = { ...spec, arch: 10 };

  it('puts turn, radius and move handles round the arc', () => {
    const handles = arcHandles(arc, 10);
    expect(handles.move[1]).toEqual(arc.centre);
    expect(handles.move[0].x).toBeCloseTo(20);
    expect(handles.move[0].y).toBeCloseTo(15);
    const [left] = handles.rotate;
    expect(Math.hypot(left.x - arc.centre.x, left.y - arc.centre.y)).toBeCloseTo(radius);
    expect(arcHandleAt(handles, handles.radius[1], 0.5)).toBe('radius');
    expect(arcHandleAt(handles, { x: 500, y: 500 }, 0.5)).toBeNull();
  });

  it('turns round the centre, in 15 degree steps with Ctrl', () => {
    const from = { x: arc.centre.x, y: arc.centre.y + 10 };
    const to = { x: arc.centre.x - 10, y: arc.centre.y + 10 };
    expect(rotateFromDrag(arc, from, to, false)).toBeCloseTo(45);
    const near = { x: arc.centre.x - 3, y: arc.centre.y + 10 };
    expect(rotateFromDrag(arc, from, near, true)).toBe(15);
  });

  it('changes the radius and keeps the centre where it is', () => {
    const from = { x: arc.centre.x, y: arc.centre.y + radius + 10 };
    const to = { x: from.x, y: from.y + 20 };
    const next = radiusFromDrag(arched, { x: 0, y: 0 }, 0, arc, from, to);
    expect(next).not.toBeNull();
    if (next === null) {
      return;
    }
    const bend = next.spec.arch ?? 0;
    expect(bend).toBeGreaterThan(0);
    expect(bend).toBeLessThan(10);
    // The radius the new bend gives, by the same chord rule.
    const a = 2 * Math.atan((2 * bend) / 40);
    const r = 20 / Math.sin(a);
    expect(r).toBeCloseTo(radius + 20);
    // Centre = origin.y + bend - r stays put.
    expect(next.origin.y + bend - r).toBeCloseTo(arc.centre.y);
    expect(radiusFromDrag(spec, { x: 0, y: 0 }, 0, arc, from, to)).toBeNull();
  });
});
