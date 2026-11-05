import { describe, expect, it } from 'vitest';

import type { EntitySummary } from '../../bridge/document';
import type { ComponentSummary } from '../../bridge/model';
import {
  boundaryFromSelection,
  dragTile,
  initialTextureState,
  nextReflect,
  seedOf,
  stateFromTexture,
  textureParams,
  tileGripAt,
  validateTexture,
} from './model';

const tile = { x: 10, y: 20, width: 30, height: 10 };

function component(patch: Partial<ComponentSummary> = {}): ComponentSummary {
  return {
    id: 5,
    name: 'Leaves',
    visible: true,
    combine: 'mergeHigh',
    heightScale: 1,
    level: 1,
    maxHeight: 2,
    ...patch,
  };
}

describe('create texture area form model', () => {
  it('starts with the tiles edge to edge, none reflected', () => {
    expect(textureParams(initialTextureState())).toEqual({
      spacingX: 0,
      spacingY: 0,
      shiftX: 0,
      shiftY: 0,
      reflect: ['none', 'none', 'none', 'none'],
      tile: null,
      angle: 0,
    });
  });

  it('cycles a tile through the four reflections', () => {
    let state = initialTextureState();
    const seen = [];
    for (let k = 0; k < 4; k += 1) {
      state = nextReflect(state, 3);
      seen.push(state.reflect[3]);
    }
    expect(seen).toEqual(['horizontal', 'vertical', 'both', 'none']);
    expect(state.reflect.slice(0, 3)).toEqual(['none', 'none', 'none']);
  });

  it('refuses spacing that folds the tiles onto themselves', () => {
    const state = { ...initialTextureState(), spacingX: '-95', shiftY: 'x' };
    expect(Object.keys(validateTexture(state)).sort()).toEqual(['shiftY', 'spacingX']);
    expect(textureParams(state)).toBeNull();
    expect(validateTexture({ ...state, spacingX: '-25', shiftY: '50' })).toEqual({});
  });

  it('tiles the picked component, or the seed of a picked texture area', () => {
    expect(seedOf(null)).toBeNull();
    expect(seedOf(component())).toBe(5);
    const texture = component({
      id: 9,
      texture: {
        seed: 5,
        params: {
          spacingX: 25,
          spacingY: -10,
          shiftX: 50,
          shiftY: 0,
          reflect: ['none', 'both', 'none', 'vertical'],
          tile,
        },
      },
    });
    expect(seedOf(texture)).toBe(5);
    expect(stateFromTexture(texture)).toMatchObject({
      name: 'Leaves',
      combine: 'mergeHigh',
      spacingX: '25',
      spacingY: '-10',
      shiftX: '50',
      reflect: ['none', 'both', 'none', 'vertical'],
      tile,
    });
  });

  it('takes the closed vectors among the selection as the boundary', () => {
    const entities = [
      { id: 1, kind: 'path', closed: true },
      { id: 2, kind: 'path', closed: false },
    ] as EntitySummary[];
    expect(boundaryFromSelection([1, 2], entities)).toEqual([1]);
  });

  it('sizes the tile from a corner and moves it from inside', () => {
    const corner = tileGripAt(tile, { x: 40.5, y: 30 }, 1);
    expect(corner).toEqual({ kind: 'corner', anchor: { x: 10, y: 20 } });
    if (corner !== null) {
      expect(dragTile(tile, corner, { x: 50, y: 40 })).toEqual({
        x: 10,
        y: 20,
        width: 40,
        height: 20,
      });
    }
    const inside = tileGripAt(tile, { x: 20, y: 25 }, 1);
    expect(inside).toEqual({ kind: 'move', from: { x: 20, y: 25 } });
    if (inside !== null) {
      expect(dragTile(tile, inside, { x: 25, y: 20 })).toEqual({ ...tile, x: 15, y: 15 });
    }
    expect(tileGripAt(tile, { x: 0, y: 0 }, 1)).toBeNull();
  });
});

describe('a turned texture area', () => {
  const tile = { x: 10, y: 0, width: 4, height: 2 };

  it('keeps the lattice turn through the form', () => {
    const state = { ...initialTextureState(), angle: 90 };
    expect(textureParams(state)?.angle).toBe(90);
  });

  it('grips and sizes the tile square to the turned lattice', () => {
    // Turned a quarter about (10, 0), the far corner (14, 2) lies at (8, 4).
    const grip = tileGripAt(tile, { x: 8, y: 4 }, 0.1, 90);
    expect(grip?.kind).toBe('corner');
    if (grip === null) {
      return;
    }
    const sized = dragTile(tile, grip, { x: 6, y: 6 }, 90);
    expect(sized.x).toBeCloseTo(10);
    expect(sized.y).toBeCloseTo(0);
    expect(sized.width).toBeCloseTo(6);
    expect(sized.height).toBeCloseTo(4);
    expect(tileGripAt(tile, { x: 9, y: 2 }, 0.1, 90)?.kind).toBe('move');
    expect(tileGripAt(tile, { x: 12, y: 1 }, 0.1, 90)).toBeNull();
  });
});
