import { describe, expect, it } from 'vitest';

import { extendPath, nearestArc, openEndNear, reverseSegments } from '../../lib/spans';
import {
  addPoint,
  clearPoints,
  closesShape,
  tangentPoint,
  commonTangent,
  moveLastPoint,
  onCircle,
  setRelative,
  toggleCurveMode,
  constrainAngle,
  initialPolylineState,
  polarPoint,
  polylinePathFromState,
  polylinePreview,
  typedPoint,
  undoPoint,
  validatePolyline,
  lineEndKey,
} from './model';

const round = (v: number) => Math.round(v * 1000) / 1000;

describe('polyline form model', () => {
  it('collects points and follows the last one in the fields', () => {
    let state = initialPolylineState('mm');
    state = addPoint(state, 'mm', { x: 10, y: 20 });
    expect(state.nextX).toBe('10');
    expect(state.nextY).toBe('20');
    state = addPoint(state, 'mm', { x: 30, y: 20 });
    expect(state.points).toHaveLength(2);
    expect(addPoint(state, 'mm', { x: 30, y: 20 }).points).toHaveLength(2);
    expect(undoPoint(state).points).toHaveLength(1);
  });

  it('needs two points for an open path and three for a closed one', () => {
    let state = addPoint(initialPolylineState('mm'), 'mm', { x: 0, y: 0 });
    expect(polylinePathFromState(state, false)).toBeNull();
    state = addPoint(state, 'mm', { x: 10, y: 0 });
    expect(polylinePathFromState(state, false)?.subpaths[0]?.closed).toBe(false);
    expect(polylinePathFromState(state, true)).toBeNull();
    state = addPoint(state, 'mm', { x: 10, y: 10 });
    expect(polylinePathFromState(state, true)?.subpaths[0]?.closed).toBe(true);
  });

  it('computes typed and polar points', () => {
    let state = { ...initialPolylineState('mm'), nextX: '5', nextY: '6' };
    expect(typedPoint(state, 'mm')).toEqual({ x: 5, y: 6 });
    expect(polarPoint(state, 'mm')).toBeNull();
    state = { ...addPoint(state, 'mm', { x: 0, y: 0 }), length: '10', angle: '90' };
    const p = polarPoint(state, 'mm');
    expect(round(p?.x ?? 1)).toBe(0);
    expect(round(p?.y ?? 0)).toBe(10);
    expect(validatePolyline({ ...state, length: '0' }, 'mm').length).toBeDefined();
  });

  it('constrains angles to 15 degree steps', () => {
    const p = constrainAngle({ x: 0, y: 0 }, { x: 10, y: 1 });
    expect(round(p.x)).toBe(10.05);
    expect(round(p.y)).toBe(0);
  });

  it('closes when clicking near the start with three or more points', () => {
    let state = addPoint(initialPolylineState('mm'), 'mm', { x: 0, y: 0 });
    state = addPoint(state, 'mm', { x: 10, y: 0 });
    expect(closesShape(state, { x: 0.1, y: 0 }, 1)).toBe(false);
    state = addPoint(state, 'mm', { x: 10, y: 10 });
    expect(closesShape(state, { x: 0.1, y: 0 }, 1)).toBe(true);
    expect(closesShape(state, { x: 5, y: 5 }, 1)).toBe(false);
  });

  it('previews the rubber segment to the cursor', () => {
    const state = addPoint(initialPolylineState('mm'), 'mm', { x: 0, y: 0 });
    expect(polylinePreview(state, null)).toEqual([]);
    expect(polylinePreview(state, { x: 4, y: 4 })[0]?.subpaths[0]?.segments).toHaveLength(1);
  });

  it('turns a dragged click into a curve and keeps curving until S', () => {
    let state = addPoint(initialPolylineState('mm'), 'mm', { x: 0, y: 0 });
    state = addPoint(state, 'mm', { x: 10, y: 0 }, { x: 0, y: 5 });
    expect(state.curveMode).toBe(true);
    state = addPoint(state, 'mm', { x: 20, y: 0 });
    expect(state.curves).toEqual([true, true]);
    state = addPoint(toggleCurveMode(state), 'mm', { x: 30, y: 0 });
    expect(state.curves).toEqual([true, true, false]);
    const segments = polylinePathFromState(state, false)?.subpaths[0]?.segments ?? [];
    expect(segments.map((s) => s.kind)).toEqual(['cubic', 'cubic', 'line']);
    // The dragged point leaves along the drag: straight up.
    const second = segments[1];
    expect(second?.kind === 'cubic' ? round(second.c1.x) : null).toBe(10);
    expect(second?.kind === 'cubic' ? second.c1.y > 0 : false).toBe(true);
    expect(clearPoints(state).curveMode).toBe(false);
    expect(undoPoint(state).curves).toEqual([true, true]);
  });

  it('types points relative to the last one', () => {
    let state = addPoint(initialPolylineState('mm'), 'mm', { x: 5, y: 5 });
    state = setRelative(state, 'mm', true);
    expect(state.nextX).toBe('0');
    expect(typedPoint({ ...state, nextX: '3', nextY: '-1' }, 'mm')).toEqual({ x: 8, y: 4 });
    expect(setRelative(state, 'mm', false).nextX).toBe('5');
  });

  it('carries an open vector on from either end', () => {
    const path = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          segments: [{ kind: 'line' as const, to: { x: 10, y: 0 } }],
          closed: false,
        },
      ],
    };
    expect(openEndNear(path, { x: 10.2, y: 0 }, 0.5)).toEqual({
      subpath: 0,
      atEnd: true,
      point: { x: 10, y: 0 },
    });
    const drawn = {
      start: { x: 10, y: 0 },
      segments: [{ kind: 'line' as const, to: { x: 10, y: 10 } }],
      closed: false,
    };
    const longer = extendPath(path, drawn, true);
    expect(longer?.subpaths[0]?.segments).toHaveLength(2);
    const before = extendPath(
      path,
      { start: { x: 0, y: 0 }, segments: [{ kind: 'line', to: { x: -5, y: 0 } }], closed: false },
      false,
    );
    expect(before?.subpaths[0]?.start).toEqual({ x: -5, y: 0 });
    expect(before?.subpaths[0]?.segments.map((s) => s.to)).toEqual([
      { x: 0, y: 0 },
      { x: 10, y: 0 },
    ]);
    const back = reverseSegments({ x: 0, y: 0 }, [
      { kind: 'cubic', c1: { x: 1, y: 1 }, c2: { x: 2, y: 1 }, to: { x: 3, y: 0 } },
    ]);
    expect(back).toEqual({
      start: { x: 3, y: 0 },
      segments: [{ kind: 'cubic', c1: { x: 2, y: 1 }, c2: { x: 1, y: 1 }, to: { x: 0, y: 0 } }],
      closed: false,
    });
  });

  it('finds the tangent from a point to an arc under the pointer', () => {
    // A half circle of radius 5 round the origin, as one bulge arc.
    const path = {
      subpaths: [
        {
          start: { x: 5, y: 0 },
          segments: [{ kind: 'arc' as const, to: { x: -5, y: 0 }, bulge: 1 }],
          closed: false,
        },
      ],
    };
    const circle = nearestArc(path, { x: 0, y: 5.2 });
    expect(round(circle?.radius ?? 0)).toBe(5);
    const touch = circle === null ? null : tangentPoint({ x: 10, y: 0 }, circle, { x: 3, y: 4 });
    // From (10, 0) the tangent touches at x = r^2 / d = 2.5, upper side.
    expect(touch === null ? null : [round(touch.x), touch.y > 0]).toEqual([2.5, true]);
    expect(circle === null ? 1 : tangentPoint({ x: 1, y: 0 }, circle, { x: 0, y: 0 })).toBeNull();
  });

  it('draws tangents starting on an arc: to a point and to another arc', () => {
    const a = { centre: { x: 0, y: 0 }, radius: 5 };
    const b = { centre: { x: 20, y: 0 }, radius: 5 };
    // Outer tangent on the top side: both touch points straight up.
    const top = commonTangent(a, b, { x: 0, y: 5 }, { x: 20, y: 5 });
    expect(top?.map((p) => [round(p.x), round(p.y)])).toEqual([
      [0, 5],
      [20, 5],
    ]);
    // Crossing tangent from the top of the first to the bottom of the second.
    const cross = commonTangent(a, b, { x: 1, y: 5 }, { x: 19, y: -5 });
    expect(cross).not.toBeNull();
    if (cross !== null) {
      const [pa, pb] = cross;
      expect(pa.y).toBeGreaterThan(0);
      expect(pb.y).toBeLessThan(0);
      expect(onCircle(pa, a, 1e-9) && onCircle(pb, b, 1e-9)).toBe(true);
      // The line is square to both radii.
      const dir = { x: pb.x - pa.x, y: pb.y - pa.y };
      expect(Math.abs(dir.x * pa.x + dir.y * pa.y)).toBeLessThan(1e-9);
    }
    // One circle inside the other has no common tangent.
    expect(commonTangent(a, { centre: { x: 1, y: 0 }, radius: 1 }, a.centre, a.centre)).toBeNull();
    // Arc to a point: the first point slides round to the touch point.
    let state = addPoint(initialPolylineState('mm'), 'mm', { x: 0, y: 5 });
    const touch = tangentPoint({ x: 20, y: 0 }, a, { x: 0, y: 5 });
    expect(touch).not.toBeNull();
    if (touch !== null) {
      state = addPoint(moveLastPoint(state, 'mm', touch), 'mm', { x: 20, y: 0 });
      expect(state.points).toHaveLength(2);
      expect(round(state.points[0]?.x ?? 0)).toBe(1.25);
    }
  });

  it('switches curve mode on a click at the last point and starts in the mode last used', () => {
    let state = addPoint(initialPolylineState('mm'), 'mm', { x: 0, y: 0 });
    expect(state.curveMode).toBe(false);
    state = addPoint(state, 'mm', { x: 0, y: 0 });
    expect(state.curveMode).toBe(true);
    expect(state.points).toHaveLength(1);
    expect(initialPolylineState('mm', true).curveMode).toBe(true);
  });
});

describe('line end keys', () => {
  it('ends on Space, closes on Tab, and leaves Enter to the Quick Keys', () => {
    expect(lineEndKey(' ')).toBe('open');
    expect(lineEndKey('Tab')).toBe('close');
    expect(lineEndKey('Enter')).toBeNull();
    expect(lineEndKey('a')).toBeNull();
  });
});
