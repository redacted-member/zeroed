import { beforeAll, describe, expect, it } from 'vitest';

import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';
import { TREE_TOOLS } from './toolbar';

beforeAll(loadExtraIcons);

describe('component tree toolbar', () => {
  it('gives every button an icon, a short label and a full name', () => {
    expect(TREE_TOOLS.map((tool) => tool.id)).toEqual([
      'moveUp',
      'moveDown',
      'newLevel',
      'group',
      'ungroup',
      'fromVisible',
      'fromVectors',
      'properties',
      'remove',
    ]);
    for (const tool of TREE_TOOLS) {
      expect(toolIcon(tool.icon), tool.id).not.toBeNull();
      expect(tool.label.length, tool.id).toBeGreaterThan(0);
      expect(tool.title.length, tool.id).toBeGreaterThanOrEqual(tool.label.length);
    }
  });
});
