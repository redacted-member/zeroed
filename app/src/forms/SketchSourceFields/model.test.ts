import { describe, expect, it } from 'vitest';

import type { EntitySummary } from '../../bridge/document';
import {
  boundaryIds,
  convertSketchSource,
  initialSketchSource,
  selectedBitmap,
  showsFilePlacement,
  sketchIds,
  sketchSourceFrom,
  sketchTrace,
  usesFile,
  validateSketchSource,
  withBoundary,
} from './model';

function entity(id: number, kind: EntitySummary['kind'], closed = true): EntitySummary {
  return { id, kind, layer: 1, bbox: [0, 0, 10, 10], closed, revision: 1 };
}

describe('sketch source', () => {
  it('asks for the bitmap to be selected until one is', () => {
    const state = initialSketchSource('mm');
    expect(usesFile(state)).toBe(true);
    expect(validateSketchSource(state, 'mm', 0).path).toBeDefined();
    const placed = { ...state, bitmap: 7 };
    expect(usesFile(placed)).toBe(false);
    expect(validateSketchSource(placed, 'mm', 0)).toEqual({});
    expect(sketchTrace(placed, 'mm', 0)).toMatchObject({
      source: 'bitmap',
      bitmap: 7,
      path: '',
      boundary: 'source',
      lineThickness: 50,
    });
  });

  it('places an image file by its corner and width, keeping proportions when the height is empty', () => {
    const state = { ...initialSketchSource('mm'), path: '/tmp/a.png', left: '10', bottom: '5' };
    expect(sketchTrace(state, 'mm', 0)).toMatchObject({
      path: '/tmp/a.png',
      origin: { x: 10, y: 5 },
      width: 100,
      height: 0,
    });
    expect(validateSketchSource({ ...state, width: '0' }, 'mm', 0).width).toBeDefined();
  });

  it('sketches the model with no file', () => {
    const state = { ...initialSketchSource('mm'), source: 'model' as const };
    expect(sketchTrace(state, 'mm', 0)).toMatchObject({ source: 'model', bitmap: null });
  });

  it('grows any boundary by the offset and limits to a picked level', () => {
    const state = { ...initialSketchSource('mm'), bitmap: 3, boundaryOffset: '4' };
    expect(sketchTrace(state, 'mm', 0)).toMatchObject({ boundary: 'source', boundaryOffset: 4 });
    expect(
      validateSketchSource({ ...state, boundaryOffset: '-1' }, 'mm', 0).boundaryOffset,
    ).toBeDefined();
    const level = { ...state, boundary: 'level' as const };
    expect(validateSketchSource(level, 'mm', 0).boundary).toBeDefined();
    const picked = sketchTrace({ ...level, level: 5 }, 'mm', 0);
    expect(picked).toMatchObject({ boundary: 'level', level: 5 });
    expect(sketchTrace({ ...state, level: 5 }, 'mm', 0)?.level).toBeNull();
    if (picked !== null) {
      expect(sketchSourceFrom(picked, 'mm').level).toBe(5);
    }
    expect(withBoundary(state, 'level', 2)).toMatchObject({ boundary: 'level', level: 2 });
    expect(withBoundary({ ...state, level: 7 }, 'level', 2).level).toBe(7);
  });

  it('needs a closed vector and a positive offset for a vector boundary', () => {
    const state = { ...initialSketchSource('mm'), bitmap: 3, boundary: 'vector' as const };
    expect(validateSketchSource(state, 'mm', 0).boundary).toBeDefined();
    expect(
      validateSketchSource({ ...state, boundaryOffset: '-1' }, 'mm', 1).boundaryOffset,
    ).toBeDefined();
    expect(sketchTrace({ ...state, boundaryOffset: '2' }, 'mm', 1)?.boundaryOffset).toBe(2);
  });

  it('finds the selected bitmap and the closed boundary vectors', () => {
    const entities = [entity(1, 'path'), entity(2, 'bitmap'), entity(3, 'path', false)];
    expect(selectedBitmap([1, 2, 3], entities)).toBe(2);
    expect(selectedBitmap([1, 3], entities)).toBeNull();
    expect(boundaryIds([1, 2, 3], entities)).toEqual([1]);
  });

  it('records the placed bitmap and, for a vector boundary, the vectors', () => {
    const state = { ...initialSketchSource('mm'), bitmap: 2 };
    expect(sketchIds(state, [1])).toEqual([2]);
    expect(sketchIds({ ...state, boundary: 'vector' }, [1])).toEqual([2, 1]);
    expect(sketchIds({ ...state, source: 'model' }, [1])).toEqual([]);
  });

  it('converts lengths with the job unit and reads settings back', () => {
    const inch = convertSketchSource(initialSketchSource('mm'), 'mm', 'inch');
    expect(Number(inch.width)).toBeCloseTo(3.937, 3);
    const trace = sketchTrace(
      { ...initialSketchSource('mm'), bitmap: 4, lineThickness: 20 },
      'mm',
      0,
    );
    expect(trace).not.toBeNull();
    if (trace !== null) {
      const back = sketchSourceFrom(trace, 'mm');
      expect(back.bitmap).toBe(4);
      expect(back.lineThickness).toBe(20);
      expect(back.height).toBe('');
    }
  });

  it('shows where a file is placed only for a toolpath saved with one', () => {
    const fresh = initialSketchSource('mm');
    expect(showsFilePlacement(fresh)).toBe(false);
    // A toolpath saved with a file keeps showing where the file is placed.
    expect(showsFilePlacement({ ...fresh, path: '/tmp/a.png' })).toBe(true);
    expect(showsFilePlacement({ ...fresh, path: '/tmp/a.png', bitmap: 3 })).toBe(false);
  });
});
