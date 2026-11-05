import { describe, expect, it } from 'vitest';

import { TOOL_GROUPS, type ToolGroup } from '../LeftPanel/toolGroups';
import { toolRows } from './model';

const every = Object.values(TOOL_GROUPS).flat();

function find(id: string): ToolGroup {
  const found = every.find((g) => g.id === id);
  if (found === undefined) {
    throw new Error(`no group ${id}`);
  }
  return found;
}

describe('toolRows', () => {
  it('captions every row of a group with more than one row', () => {
    const rows = toolRows(find('createVectors'));
    expect(rows.map((r) => r.caption)).toEqual([
      'Shapes',
      'Lines, curves, texture and dimensions',
      'Text and tracing',
    ]);
    expect(rows[0]?.tools.map((t) => t.id)).toEqual([
      'drawCircle',
      'drawEllipse',
      'drawRectangle',
      'drawPolygon',
      'drawStar',
    ]);
  });

  it('leaves a single row to its group title', () => {
    expect(toolRows(find('offsetAndLayout')).map((r) => r.caption)).toEqual([undefined]);
  });

  it('has a caption for the first tool of every row in every multi row group', () => {
    for (const group of every.filter((g) => toolRows(g).length > 1)) {
      for (const row of toolRows(group)) {
        expect(row.caption, `${group.id}/${row.tools[0]?.id}`).toBeDefined();
      }
    }
  });

  it('keeps every row within the six columns of the grid', () => {
    for (const group of every) {
      for (const row of toolRows(group)) {
        expect(row.tools.length, `${group.id}/${row.tools[0]?.id}`).toBeLessThanOrEqual(6);
      }
    }
  });
});
