import { describe, expect, it } from 'vitest';

import { toolDiameter, type Tool } from './cam';

function tool(toolType: Tool['toolType'], diameter?: number): Tool {
  return { toolType, ...(diameter === undefined ? {} : { diameter }) } as Tool;
}

describe('tool diameter', () => {
  it('reads the list diameter, then the geometry, then a laser kerf', () => {
    expect(toolDiameter(tool({ kind: 'endMill', diameter: 6 }, 6.35))).toBe(6.35);
    expect(toolDiameter(tool({ kind: 'endMill', diameter: 6 }))).toBe(6);
    // A toolpath's own copy of a laser tool has no list diameter.
    expect(toolDiameter(tool({ kind: 'laserTool', power: 80, speed: 1200, kerf: 0.1 }))).toBe(0.1);
    expect(toolDiameter(tool({ kind: 'formTool' }))).toBe(0);
  });
});
