import { describe, expect, it } from 'vitest';

import { DEFAULT_PREFS, mergePrefs } from '../../lib/prefs';
import {
  draftFrom,
  draftPatch,
  draftProblems,
  editDraft,
  numberProblem,
  shownRange,
  withDefaultUnits,
} from './model';

describe('preferences dialog model', () => {
  it('shows numbers as text and applies them back', () => {
    const draft = draftFrom(DEFAULT_PREFS);
    expect(draft.numbers.undoLimit).toBe('200');
    expect(draft.numbers.tolerance).toBe('0.001');
    const edited = { ...draft, numbers: { ...draft.numbers, undoLimit: ' 40 ', gridSpacing: '1' } };
    const patch = draftPatch(edited);
    expect(patch?.general?.undoLimit).toBe(40);
    expect(patch?.view2d?.gridSpacing).toBeCloseTo(25.4, 9);
  });

  it('shows lengths in the default units and converts them when those change', () => {
    const metric = mergePrefs(DEFAULT_PREFS, {
      general: { defaultUnits: 'mm' },
      toolpaths: { tolerance: 0.025 },
    });
    const draft = draftFrom(metric);
    expect(draft.numbers.tolerance).toBe('0.025');
    expect(draft.numbers.gridSpacing).toBe('12.7');
    expect(draftPatch(draft)?.view2d?.gridSpacing).toBe(12.7);
    const inches = withDefaultUnits(draft, 'inch');
    expect(inches.numbers.gridSpacing).toBe('0.5');
    expect(inches.numbers.tolerance).toBe('0.0009843');
    expect(inches.numbers.undoLimit).toBe('200');
    // Left alone, the stored millimeters stay exact.
    expect(draftPatch(inches)?.toolpaths?.tolerance).toBe(0.025);
    const typed = { ...inches, numbers: { ...inches.numbers, gridSpacing: '2' } };
    expect(withDefaultUnits(typed, 'mm').numbers.gridSpacing).toBe('50.8');
  });

  it('checks a length against its limits in the units it is typed in', () => {
    expect(numberProblem('tolerance', '0.05', 'inch')).toBe('range');
    expect(numberProblem('tolerance', '0.03', 'mm')).toBeNull();
    expect(shownRange('tolerance', 'mm').max).toBe('1');
    expect(shownRange('tolerance', 'inch').max).toBe('0.03937');
  });

  it('refuses numbers that are not numbers, not whole or out of range', () => {
    expect(numberProblem('undoLimit', 'abc')).toBe('number');
    expect(numberProblem('undoLimit', '')).toBe('number');
    expect(numberProblem('undoLimit', '2.5')).toBe('whole');
    expect(numberProblem('undoLimit', '0')).toBe('range');
    expect(numberProblem('tolerance', '0.01')).toBeNull();
    const draft = draftFrom(DEFAULT_PREFS);
    const bad = { ...draft, numbers: { ...draft.numbers, snapRadiusPx: '99' } };
    expect(draftProblems(bad)).toEqual({ snapRadiusPx: 'range' });
    expect(draftPatch(bad)).toBeNull();
  });

  it('leaves the recent files list alone unless it was cleared', () => {
    const prefs = mergePrefs(DEFAULT_PREFS, { general: { recentFiles: ['/a.carve'] } });
    const draft = draftFrom(prefs);
    expect(draftPatch(draft)?.general).not.toHaveProperty('recentFiles');
    expect(draftPatch({ ...draft, clearRecent: true })?.general?.recentFiles).toEqual([]);
  });

  it('carries the toolpath list and geometry fixing options through', () => {
    const draft = draftFrom(DEFAULT_PREFS);
    expect(draft.numbers.geometryFixSeconds).toBe('10');
    expect(numberProblem('geometryFixSeconds', '0')).toBeNull();
    expect(numberProblem('geometryFixSeconds', '1.5')).toBe('whole');
    expect(numberProblem('geometryFixSeconds', '4000')).toBe('range');
    const edited = editDraft(
      { ...draft, numbers: { ...draft.numbers, geometryFixSeconds: '30' } },
      'toolpaths',
      { checkAllDepths: true, appendDuplicated: true, appendToSelectedGroup: false },
    );
    const patch = draftPatch(edited);
    expect(patch?.toolpaths?.geometryFixSeconds).toBe(30);
    expect(patch?.toolpaths?.checkAllDepths).toBe(true);
    expect(patch?.toolpaths?.appendDuplicated).toBe(true);
    expect(patch?.toolpaths?.appendToSelectedGroup).toBe(false);
  });

  it('edits one section without touching the others', () => {
    const draft = editDraft(draftFrom(DEFAULT_PREFS), 'view3d', { shading: 'flat' });
    expect(draft.prefs.view3d.shading).toBe('flat');
    expect(draft.prefs.view3d.background).toBe('theme');
    expect(draftPatch(draft)?.view3d?.shading).toBe('flat');
    expect(draft.prefs.general).toEqual(DEFAULT_PREFS.general);
  });
});
