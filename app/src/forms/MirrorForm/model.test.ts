import { beforeAll, describe, expect, it } from 'vitest';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';

import { applyMatrix } from '../../lib/geometry';
import {
  MIRROR_BUTTONS,
  initialMirrorState,
  mirrorAcross,
  mirrorLine,
  mirrorMatrixFor,
} from './model';

const round = (p: { x: number; y: number } | null) =>
  p === null ? null : { x: Math.round(p.x * 1e6) / 1e6, y: Math.round(p.y * 1e6) / 1e6 };

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('mirror form model', () => {
  it('offers the flips and starts without copy, on rotated bounds', () => {
    expect(MIRROR_BUTTONS.map((b) => b.mode)).toEqual([
      'horizontal',
      'vertical',
      'left',
      'right',
      'top',
      'bottom',
      'materialHorizontal',
      'materialVertical',
    ]);
    expect(initialMirrorState()).toEqual({ copy: false, rotatedBounds: true });
  });

  it('returns null without a selection and a flip with one', () => {
    expect(mirrorMatrixFor(null, 'horizontal')).toBeNull();
    const m = mirrorMatrixFor([0, 0, 10, 10], 'right');
    expect(m === null ? null : applyMatrix(m, { x: 0, y: 5 })).toEqual({ x: 20, y: 5 });
  });

  it('flips about the material centre and about a rotated box', () => {
    const m = mirrorMatrixFor([0, 0, 10, 10], 'materialHorizontal', { material: [0, 0, 100, 50] });
    expect(m === null ? null : applyMatrix(m, { x: 10, y: 5 })).toEqual({ x: 90, y: 5 });
    // A box turned 90 degrees: its own horizontal flip is a job vertical flip.
    const frame = { angle: 90, extent: [0, -10, 20, 0] as [number, number, number, number] };
    const turned = mirrorMatrixFor([0, 0, 10, 20], 'horizontal', { frame, rotatedBounds: true });
    expect(round(turned === null ? null : applyMatrix(turned, { x: 5, y: 0 }))).toEqual({
      x: 5,
      y: 20,
    });
  });

  it('mirrors across a picked straight line', () => {
    const line = mirrorLine({
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: false,
          segments: [{ kind: 'line', to: { x: 10, y: 10 } }],
        },
      ],
    });
    expect(line).not.toBeNull();
    const m = line === null ? null : mirrorAcross(line);
    expect(round(m === null ? null : applyMatrix(m, { x: 5, y: 0 }))).toEqual({ x: 0, y: 5 });
    expect(mirrorLine({ subpaths: [] })).toBeNull();
  });

  it('draws every mode as an icon button with a short label inside its full name', () => {
    expect(MIRROR_BUTTONS.filter((b) => toolIcon(b.icon) === null)).toEqual([]);
    expect(
      MIRROR_BUTTONS.filter((b) => !b.label.toLowerCase().includes(b.short.toLowerCase())),
    ).toEqual([]);
    expect(new Set(MIRROR_BUTTONS.map((b) => b.icon)).size).toBe(MIRROR_BUTTONS.length);
  });
});
