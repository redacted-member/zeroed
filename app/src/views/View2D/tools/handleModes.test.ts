import { describe, expect, it } from 'vitest';

import {
  centreAfterTransform,
  handlesAfterMove,
  handlesAfterPick,
  handlesAfterReclick,
  rotationCentre,
  rotationStep,
  scaleFactors,
} from './handleModes';

describe('selection handles', () => {
  it('shows no handles on a first pick in Vector Selection mode', () => {
    expect(handlesAfterPick(false)).toBe('none');
  });

  it('shows the handles at once in Interactive Selection mode', () => {
    expect(handlesAfterPick(true)).toBe('scale');
  });

  it('brings up the handles on a second click, then toggles the rotational anchor', () => {
    expect(handlesAfterReclick('none')).toBe('scale');
    expect(handlesAfterReclick('scale')).toBe('pivot');
    expect(handlesAfterReclick('pivot')).toBe('scale');
  });

  it('keeps the handles a move started with', () => {
    expect(handlesAfterMove(false, 'none')).toBe('none');
    expect(handlesAfterMove(false, 'pivot')).toBe('pivot');
    expect(handlesAfterMove(true, 'none')).toBe('scale');
  });
});

describe('handle drags', () => {
  const none = { alt: false, shift: false };

  it('scales a corner in proportion unless Alt is held', () => {
    expect(scaleFactors('corner', 2, 1.5, none)).toEqual({ sx: 2, sy: 2 });
    expect(scaleFactors('corner', -2, 1.5, none)).toEqual({ sx: -2, sy: 2 });
    expect(scaleFactors('corner', 2, 1.5, { alt: true, shift: false })).toEqual({ sx: 2, sy: 1.5 });
  });

  it('scales an edge along its own axis, both with Shift', () => {
    expect(scaleFactors('horizontalEdge', 2, 9, none)).toEqual({ sx: 2, sy: 1 });
    expect(scaleFactors('verticalEdge', 9, 3, none)).toEqual({ sx: 1, sy: 3 });
    expect(scaleFactors('horizontalEdge', 2, 9, { alt: false, shift: true })).toEqual({
      sx: 2,
      sy: 2,
    });
  });

  it('rotates in 15 degree steps with Alt', () => {
    expect(rotationStep(22, true)).toBe(15);
    expect(rotationStep(23, true)).toBe(30);
    expect(rotationStep(22, false)).toBe(22);
  });
});

describe('rotation center', () => {
  const frame = { angle: 0, extent: [0, 0, 40, 20] as [number, number, number, number] };

  it('sits in the middle of the selection until it is moved', () => {
    expect(rotationCentre(frame, null)).toEqual({ x: 20, y: 10 });
    expect(rotationCentre(frame, { x: 3, y: 4 })).toEqual({ x: 3, y: 4 });
  });

  it('follows the middle of a rotated frame', () => {
    const turned = { angle: 90, extent: [0, 0, 40, 20] as [number, number, number, number] };
    const centre = rotationCentre(turned, null);
    expect(centre.x).toBeCloseTo(-10);
    expect(centre.y).toBeCloseTo(20);
  });

  it('moves with the selection once it has been placed', () => {
    expect(centreAfterTransform(null, [1, 0, 0, 1, 5, 5])).toBeNull();
    expect(centreAfterTransform({ x: 1, y: 2 }, [2, 0, 0, 2, 5, 5])).toEqual({ x: 7, y: 9 });
  });
});
