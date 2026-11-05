import { describe, expect, it } from 'vitest';

import type { EntitySummary, Path } from '../../bridge/document';
import { polylinePath } from '../../lib/geometry';
import { railOf, type Rail } from '../TwoRailSweepForm/rails';
import {
  addToAllNodes,
  attachSection,
  componentToUse,
  deleteStation,
  extrudeArgs,
  extrudeOverlays,
  initialExtrudeState,
  moveStation,
  railsFromSelection,
  removeAllOn,
  reverseRail,
  sectionsToUse,
  setStart,
  toggleSmooth,
  validateExtrude,
  withRails,
  type ExtrudeFormState,
} from './model';

const mm = initialExtrudeState('mm');

function rail(path: Path): Rail {
  const r = railOf(path, false, null);
  if (r === null) {
    throw new Error('no rail');
  }
  return r;
}

const straight = polylinePath(
  [
    { x: 0, y: 0 },
    { x: 50, y: 0 },
    { x: 100, y: 0 },
  ],
  false,
);
const square = polylinePath(
  [
    { x: 0, y: 0 },
    { x: 40, y: 0 },
    { x: 40, y: 40 },
    { x: 0, y: 40 },
  ],
  true,
);

function entity(id: number, closed: boolean, kind = 'path'): EntitySummary {
  return { id, kind, layer: 1, bbox: [0, 0, 1, 1], closed, revision: 1 } as EntitySummary;
}

describe('extrude and weave form', () => {
  it('starts with vector sections, no weave and 70 / 130 percent', () => {
    expect(mm.source).toBe('vectors');
    expect(mm.weave).toBe(false);
    expect([mm.zUnder, mm.zOver]).toEqual(['70', '130']);
    expect(mm.fill).toBe('stretch');
  });

  it('takes one or more vectors as rails, and new rails clear the sections', () => {
    const entities = [entity(1, false), entity(2, true), entity(3, false, 'bitmap')];
    expect(railsFromSelection([2, 1], entities)).toEqual([2, 1]);
    expect(railsFromSelection([1, 3], entities)).toBeNull();
    expect(railsFromSelection([], entities)).toBeNull();
    const withStations: ExtrudeFormState = {
      ...mm,
      rails: [1],
      stations: [{ rail: 0, section: 9, at: { x: 0, y: 0 }, smooth: true }],
    };
    const next = withRails(withStations, [4, 5]);
    expect(next.stations).toEqual([]);
    expect(next.reverse).toEqual([false, false]);
    expect(reverseRail(next, 1).reverse).toEqual([false, true]);
    expect(setStart(next, 0, { x: 3, y: 4 }).starts).toEqual([{ x: 3, y: 4 }, null]);
  });

  it('attaches the first section at both ends of an open rail, later ones at the click', () => {
    const rails = [rail(straight)];
    let s = attachSection({ ...mm, rails: [1] }, rails, 0, 7, { x: 30, y: 1 });
    expect(s.stations.map((st) => st.at.x)).toEqual([0, 100]);
    s = attachSection(s, rails, 0, 8, { x: 30, y: 1 });
    expect(s.stations.map((st) => [st.section, st.at.x])).toEqual([
      [7, 0],
      [8, 30],
      [7, 100],
    ]);
    expect(sectionsToUse(s, [])).toEqual([7, 8]);
    s = moveStation(s, rails, 1, { x: 60, y: 5 });
    expect(s.stations[1]?.at).toEqual({ x: 60, y: 0 });
    expect(toggleSmooth(s, 1).stations[1]?.smooth).toBe(false);
    expect(deleteStation(s, 0).stations).toHaveLength(2);
    expect(removeAllOn(s, 0).stations).toEqual([]);
    const closed = attachSection({ ...mm, rails: [2] }, [rail(square)], 0, 7, { x: 5, y: 0 });
    expect(closed.stations).toHaveLength(1);
  });

  it('adds a section at every node of its rail', () => {
    const rails = [rail(straight)];
    const s = attachSection({ ...mm, rails: [1] }, rails, 0, 7, { x: 0, y: 0 });
    const all = addToAllNodes(s, rails, 0);
    expect(all.stations.map((st) => st.at.x)).toEqual([0, 50, 100]);
  });

  it('uses a lone selected open vector as the section when none is attached', () => {
    const s = { ...mm, rails: [1] };
    expect(sectionsToUse(s, [1, 6])).toEqual([6]);
    expect(sectionsToUse(s, [6, 7])).toEqual([]);
  });

  it('builds the kernel payload with stations as fractions of each rail', () => {
    const rails = [rail(straight)];
    let s = attachSection({ ...mm, rails: [1], reverse: [false], starts: [null] }, rails, 0, 7, {
      x: 0,
      y: 0,
    });
    s = attachSection(s, rails, 0, 8, { x: 25, y: 0 });
    const args = extrudeArgs({ ...s, sharpCorners: true }, rails, [], [], 'mm');
    expect(args?.sections).toEqual([7, 8]);
    expect(args?.params.stations.map((st) => [st.section, st.at])).toEqual([
      [0, 0],
      [1, 0.25],
      [0, 1],
    ]);
    expect(args?.params.sharpCorners).toBe(true);
    expect(args?.component).toBeUndefined();
    expect(extrudeArgs({ ...mm, rails: [1] }, rails, [], [], 'mm')).toBeNull();
  });

  it('carries the chosen component, or the first listed', () => {
    const components = [{ id: 4 }, { id: 9 }];
    const s: ExtrudeFormState = { ...mm, rails: [1], source: 'component', component: 9 };
    expect(componentToUse(s, components)).toBe(9);
    expect(componentToUse({ ...s, component: 99 }, components)).toBe(4);
    const args = extrudeArgs(
      { ...s, fill: 'copies', overlap: '25', flipAlternate: true },
      [rail(straight)],
      [],
      components,
      'mm',
    );
    expect(args?.component).toBe(9);
    expect(args?.sections).toEqual([]);
    expect(args?.params.fill).toBe('copies');
    expect(args?.params.overlap).toBe(25);
    expect(args?.params.flipAlternate).toBe(true);
    expect(extrudeArgs(s, [rail(straight)], [], [], 'mm')).toBeNull();
  });

  it('checks the overlap, the weave percentages and the exact height', () => {
    const copies: ExtrudeFormState = { ...mm, source: 'component', fill: 'copies' };
    expect(validateExtrude({ ...copies, overlap: '96' }, 'mm').overlap).toBeDefined();
    expect(validateExtrude({ ...copies, overlap: '50' }, 'mm').overlap).toBeUndefined();
    expect(validateExtrude({ ...mm, weave: true, zUnder: '-1' }, 'mm').zUnder).toBeDefined();
    expect(validateExtrude({ ...mm, zUnder: '-1' }, 'mm').zUnder).toBeUndefined();
    expect(validateExtrude({ ...mm, exactHeight: true, height: '0' }, 'mm').height).toBeDefined();
    const woven = extrudeArgs(
      { ...mm, rails: [1], weave: true, weaveMode: 'addBase', zUnder: '0', zOver: '50' },
      [rail(straight)],
      [6],
      [],
      'mm',
    );
    expect(woven?.params).toMatchObject({
      weave: true,
      weaveMode: 'addBase',
      zUnder: 0,
      zOver: 50,
    });
  });

  it('draws each rail with its start and a marker where each section hangs', () => {
    const rails = [rail(straight)];
    const s = attachSection({ ...mm, rails: [1] }, rails, 0, 7, { x: 0, y: 0 });
    const overlays = extrudeOverlays(s, rails, [straight], 1);
    expect(overlays.filter((o) => o.tone === 'accent')).toHaveLength(4);
    expect(overlays[0]?.path).toBe(straight);
  });
});
