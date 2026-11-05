import { describe, expect, it } from 'vitest';

import { strings } from '../../lib/formStrings';
import { usageLines } from './model';

const t = strings.usageInfo;

describe('3D usage information', () => {
  it('follows the navigation scheme', () => {
    expect(usageLines('navigate', 'modern', 'Cmd')).toEqual([t.rotate, t.panModern, t.zoomModern]);
    expect(usageLines('navigate', 'legacy', 'Cmd')).toEqual([t.rotate, t.panLegacy, t.zoomLegacy]);
  });

  it('adds the component moves over a model and lists the sculpting and handle controls', () => {
    expect(usageLines('model', 'modern', 'Ctrl')).toContain(t.moveComponent('Ctrl'));
    expect(usageLines('sculpt', 'modern', 'Ctrl')[0]).toBe(t.sculpt);
    expect(usageLines('transform', 'legacy', 'Ctrl')).toHaveLength(3);
  });

  it('lists the slice height controls while Slice Model is open', () => {
    const lines = usageLines('slice', 'modern', 'Ctrl');
    expect(lines).toEqual([
      t.rotate,
      t.panModern,
      t.zoomModern,
      t.sliceMove,
      t.sliceInsert,
      t.sliceNudge,
    ]);
    expect(lines).not.toContain(t.editComponent);
  });
});
