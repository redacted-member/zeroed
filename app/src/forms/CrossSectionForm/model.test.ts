import { describe, expect, it } from 'vitest';

import { clickPoint, INITIAL_CROSS_SECTION, sectionPreview } from './model';

describe('cross section model', () => {
  it('makes the section on the second click and starts over', () => {
    const one = clickPoint(INITIAL_CROSS_SECTION, { x: 10, y: 20 });
    expect(one.section).toBeNull();
    expect(one.state.start).toEqual({ x: 10, y: 20 });
    const two = clickPoint(one.state, { x: 110, y: 20 });
    expect(two.section).toEqual({ from: { x: 10, y: 20 }, to: { x: 110, y: 20 } });
    expect(two.state).toEqual(INITIAL_CROSS_SECTION);
  });

  it('sends no typed ends or height scale', () => {
    const one = clickPoint(INITIAL_CROSS_SECTION, { x: 0, y: 0 });
    const two = clickPoint(one.state, { x: 5, y: 0 });
    expect(Object.keys(two.section ?? {}).sort()).toEqual(['from', 'to']);
  });

  it('waits for a second click away from the first', () => {
    const one = clickPoint(INITIAL_CROSS_SECTION, { x: 1, y: 1 });
    const same = clickPoint(one.state, { x: 1, y: 1 });
    expect(same.section).toBeNull();
    expect(same.state.start).toEqual({ x: 1, y: 1 });
  });

  it('previews the line only once it has a start', () => {
    expect(sectionPreview(null, null)).toEqual([]);
    const [line] = sectionPreview({ x: 0, y: 0 }, { x: 5, y: 0 });
    expect(line?.subpaths[0]?.segments).toHaveLength(1);
  });
});
