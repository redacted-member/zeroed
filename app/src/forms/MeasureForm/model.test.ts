import { describe, expect, it } from 'vitest';

import type { MeasureHit } from '../../bridge/drawTools';
import {
  EMPTY_MEASURE,
  measureMove,
  measurePress,
  measureRelease,
  pointRows,
  rulerPreview,
  spanPath,
  spanRows,
  startsNewMeasurement,
  contourRows,
} from './model';

const a = { x: 0, y: 0 };
const b = { x: 30, y: 40 };

describe('Measure tool model', () => {
  it('measures with two clicks and follows the pointer in between', () => {
    let state = measurePress(EMPTY_MEASURE, a);
    expect(startsNewMeasurement(EMPTY_MEASURE)).toBe(true);
    state = measureRelease(state, a, false);
    expect(state.fixed).toBe(false);
    state = measureMove(state, { x: 3, y: 4 });
    expect(pointRows(state, null, 'mm').find((r) => r.label === 'Distance')?.value).toBe('5 mm');
    expect(startsNewMeasurement(state)).toBe(false);
    state = measurePress(state, b);
    expect(state).toEqual({ from: a, to: b, fixed: true });
    expect(measureMove(state, { x: 1, y: 1 })).toBe(state);
    expect(startsNewMeasurement(state)).toBe(true);
    expect(measurePress(state, b)).toEqual({ from: b, to: b, fixed: false });
  });

  it('fixes the second point where a drag ends', () => {
    const state = measureRelease(measurePress(EMPTY_MEASURE, a), b, true);
    expect(state).toEqual({ from: a, to: b, fixed: true });
    expect(rulerPreview(state)[0]?.subpaths[0]?.segments[0]).toEqual({ kind: 'line', to: b });
    expect(rulerPreview(measurePress(EMPTY_MEASURE, a))).toEqual([]);
  });

  it('reads out distance, angle and deltas in the job units', () => {
    const rows = pointRows({ from: a, to: b, fixed: true }, { x: 1, y: 1 }, 'mm');
    expect(rows.map((row) => row.value)).toEqual([
      '0, 0 mm',
      '30, 40 mm',
      '50 mm',
      '53.13 deg',
      '30 mm',
      '40 mm',
    ]);
    expect(rows[0]?.label).toBe('First Point');
    expect(rows[1]?.label).toBe('Cursor Position');
    const inch = pointRows({ from: a, to: { x: 25.4, y: 0 }, fixed: true }, null, 'inch');
    expect(inch.find((r) => r.label === 'Distance')?.value).toBe('1 inch');
    expect(pointRows(EMPTY_MEASURE, null, 'mm')).toEqual([]);
    expect(pointRows(EMPTY_MEASURE, { x: 2, y: 3 }, 'mm')).toEqual([
      { label: 'Cursor Position', value: '2, 3 mm' },
    ]);
  });

  it('reads a span and a contour', () => {
    const hit: MeasureHit = {
      id: 3,
      point: { x: 0, y: 10 },
      distance: 0.2,
      length: 62.83185,
      closed: true,
      width: 20,
      height: 20,
      area: 314.159,
      spans: 2,
      path: {
        subpaths: [
          {
            start: { x: 10, y: 0 },
            closed: true,
            segments: [
              { kind: 'arc', to: { x: -10, y: 0 }, bulge: -1 },
              { kind: 'arc', to: { x: 10, y: 0 }, bulge: -1 },
            ],
          },
        ],
      },
      segment: {
        kind: 'arc',
        length: 31.4159,
        start: { x: 10, y: 0 },
        end: { x: -10, y: 0 },
        radius: 10,
        centre: { x: 0, y: 0 },
        sweepDeg: -180,
      },
    };
    const span = new Map(spanRows(hit, 'mm').map((row) => [row.label, row.value]));
    expect(span.get('Type')).toBe('Arc');
    expect(span.get('Length')).toBe('31.416 mm');
    expect(span.get('Start Point')).toBe('10, 0 mm');
    expect(span.get('End Point')).toBe('-10, 0 mm');
    expect(span.get('Radius')).toBe('10 mm');
    expect(spanPath(hit).subpaths[0]?.segments[0]?.kind).toBe('arc');
    const contour = new Map(contourRows(hit, 'mm').map((row) => [row.label, row.value]));
    expect(contour.get('Area')).toBe('314.159 sq mm');
    expect(contour.get('Perimeter')).toBe('62.832 mm');
    expect(contour.get('Num. Spans')).toBe('2');
    const open = contourRows({ ...hit, closed: false }, 'mm');
    expect(open[0]?.value).toBe('Open vector, no area');
  });
});
