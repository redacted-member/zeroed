import { describe, expect, it } from 'vitest';

import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';
import { menuItems } from '../PopupMenu/model';
import { faceMenuEntries } from './model';

describe('faceMenuEntries', () => {
  it('offers Add Model to Face, then Edit Face in 2D and 3D View, in that order', () => {
    expect(menuItems(faceMenuEntries()).map((i) => [i.label, i.action])).toEqual([
      ['Add Model to Face...', 'addModel'],
      ['Edit Face in 2D View', 'edit2d'],
      ['Edit Face in 3D View', 'edit3d'],
    ]);
  });

  it('draws an icon beside every item', async () => {
    await loadExtraIcons();
    for (const item of menuItems(faceMenuEntries())) {
      expect(item.icon, item.id).toBeDefined();
      expect(toolIcon(item.icon ?? ''), item.id).not.toBeNull();
    }
  });
});
