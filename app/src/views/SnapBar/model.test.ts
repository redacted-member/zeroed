import { describe, expect, it } from 'vitest';

import type { SnapOptions } from '../../bridge/document';
import { activePicker, groupsThatFit, levelPicker, objectSnapToggle, viewToolTip } from './model';

const NONE: SnapOptions = {
  grid: true,
  guides: true,
  nodes: false,
  midpoints: false,
  centres: false,
  intersections: false,
  jobPoints: false,
  smart: true,
};

describe('objectSnapToggle', () => {
  it('turns every object snap on when none is on', () => {
    expect(objectSnapToggle(NONE)).toEqual({
      nodes: true,
      midpoints: true,
      centres: true,
      intersections: true,
      jobPoints: true,
    });
  });

  it('turns them all off when any is on, leaving grid, guides and smart alone', () => {
    const patch = objectSnapToggle({ ...NONE, centres: true });
    expect(patch).toEqual({
      nodes: false,
      midpoints: false,
      centres: false,
      intersections: false,
      jobPoints: false,
    });
    expect('grid' in patch).toBe(false);
  });
});

describe('view toolbar drop-downs', () => {
  it('chooses the active sheet or layer and disables a list of one', () => {
    const one = activePicker([{ id: 1, name: 'Sheet 1', active: true }]);
    expect(one).toEqual({ options: [{ id: 1, name: 'Sheet 1' }], value: 1, disabled: true });
    const two = activePicker([
      { id: 1, name: 'Layer 1', active: false },
      { id: 4, name: 'Cut', active: true },
    ]);
    expect(two.value).toBe(4);
    expect(two.disabled).toBe(false);
  });

  it('hides the level drop-down without levels and lists the top level first', () => {
    expect(levelPicker([], null)).toEqual({ options: [], value: null, disabled: true });
    const picker = levelPicker(
      [
        { id: 1, name: 'Level 1' },
        { id: 2, name: 'Level 2' },
      ],
      null,
    );
    expect(picker?.options.map((o) => o.id)).toEqual([2, 1]);
    expect(picker?.value).toBe(1);
  });
});

describe('toolbar overflow', () => {
  it('keeps every group while they fit', () => {
    expect(groupsThatFit([100, 200, 50], 350, 30)).toBe(3);
  });

  it('moves the groups past the edge behind the More button', () => {
    expect(groupsThatFit([100, 200, 50], 340, 30)).toBe(2);
    expect(groupsThatFit([100, 200, 50], 200, 30)).toBe(1);
    expect(groupsThatFit([100, 200, 50], 90, 30)).toBe(0);
  });

  it('shows everything until every width is known', () => {
    expect(groupsThatFit([100, undefined, 50], 120, 30)).toBe(3);
  });
});

describe('viewToolTip', () => {
  it('names the key that picks the zoom box', () => {
    expect(viewToolTip('zoomBox')).toMatch(/^Zoom.* \(Z\)$/i);
    expect(viewToolTip('pan')).not.toContain('(');
  });
});
