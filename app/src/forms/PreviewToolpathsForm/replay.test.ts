import { describe, expect, it } from 'vitest';

import { recordRun, recordWaste, replayPlan, undoStep } from './replay';

describe('making a preview again', () => {
  it('adds a kept run to the preview shown and starts again otherwise', () => {
    const first = recordRun(null, 'standard', { ids: [1, 2], keep: false }, false);
    const kept = recordRun(first, 'standard', { ids: [3], keep: true }, true);
    expect(kept.steps.map((s) => s.kind === 'run' && s.ids)).toEqual([[1, 2], [3]]);
    // Nothing shown, or another quality: the kernel cuts fresh material.
    expect(recordRun(first, 'standard', { ids: [3], keep: true }, false).steps).toHaveLength(1);
    const other = recordRun(kept, 'high', { ids: [3], keep: true }, true);
    expect(other.steps).toEqual([{ kind: 'run', ids: [3], keep: false }]);
    // A run into fresh material starts the history again.
    expect(recordRun(kept, 'standard', { keep: false }, true).steps).toHaveLength(1);
  });

  it('keeps waste removal and takes the last step back on undo', () => {
    const first = recordRun(null, 'standard', { keep: false }, false);
    const waste = recordWaste(first, 10, 20);
    expect(waste?.steps[1]).toEqual({ kind: 'waste', x: 10, y: 20 });
    expect(undoStep(waste)).toEqual(first);
    expect(undoStep(first)).toBeNull();
    expect(recordWaste(null, 1, 2)).toBeNull();
  });

  it('animates the last run that cut the toolpath, the rest before and after it', () => {
    let history = recordRun(null, 'standard', { ids: [1, 2], keep: false }, false);
    history = recordWaste(history, 5, 5) ?? history;
    history = recordRun(history, 'standard', { ids: [3], keep: true }, true);
    const early = replayPlan(history, 1);
    expect(early?.before).toEqual([]);
    expect(early?.run.ids).toEqual([1, 2]);
    expect(early?.after.map((s) => s.kind)).toEqual(['waste', 'run']);
    const late = replayPlan(history, 3);
    expect(late?.before.map((s) => s.kind)).toEqual(['run', 'waste']);
    expect(late?.run).toEqual({ kind: 'run', ids: [3], keep: true });
    expect(late?.after).toEqual([]);
    // A toolpath no run names: the last run.
    expect(replayPlan(history, 9)?.run.ids).toEqual([3]);
    // The visible toolpaths are taken to include any.
    const visible = recordRun(null, 'standard', { keep: false }, false);
    expect(replayPlan(visible, 4)?.run.ids).toBeUndefined();
    expect(replayPlan(null, 1)).toBeNull();
  });
});
