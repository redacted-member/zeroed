import { describe, expect, it } from 'vitest';

import type { PostSummary, ToolpathSummary } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { immediateFileName, immediateProblem, immediateSaveArgs, immediateToolpath } from './model';

const engrave: ToolpathSummary = {
  id: 4,
  name: 'Quick Engrave 1',
  kind: 'engrave',
  visible: true,
  stale: false,
  toolName: 'Engraver',
  toolNumber: 2,
  estimatedSeconds: 0,
  moves: 10,
};

const grbl: PostSummary = {
  id: 'grbl_mm',
  name: 'Grbl (mm)',
  fileExtension: 'gcode',
  units: 'mm',
  arcs: true,
};

describe('Immediate Output', () => {
  it('saves the toolpath the form calculated last', () => {
    expect(immediateToolpath([engrave], 4)).toBe(engrave);
    expect(immediateToolpath([engrave], null)).toBeNull();
    expect(immediateToolpath([], 4)).toBeNull();
  });

  it('is greyed until a toolpath is calculated and a post is picked', () => {
    expect(immediateProblem(null, grbl)).toBe(strings.toolpaths.immediate.notCalculated);
    expect(immediateProblem(engrave, null)).toBe(strings.toolpaths.immediate.noPost);
    expect(immediateProblem(engrave, grbl)).toBeNull();
  });

  it('refuses a laser toolpath through a post that cannot drive a laser', () => {
    expect(immediateProblem({ ...engrave, kind: 'laser' }, grbl)).toContain('laser');
  });

  it('offers the toolpath name, with the side when asked', () => {
    expect(immediateFileName(engrave, grbl, null)).toBe('Quick Engrave 1.gcode');
    expect(immediateFileName(engrave, grbl, 'Top')).toBe('Quick Engrave 1 (Top).gcode');
  });

  it('writes only that toolpath', () => {
    expect(immediateSaveArgs(engrave, grbl, '/out/a.gcode')).toEqual({
      toolpathIds: [4],
      post: 'grbl_mm',
      path: '/out/a.gcode',
      output: 'selected',
    });
  });
});
