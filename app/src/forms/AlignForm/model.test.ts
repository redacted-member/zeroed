import { beforeAll, describe, expect, it } from 'vitest';

import { TOOL_GROUPS } from '../../shell/LeftPanel/toolGroups';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';
import { spaceInOrder, spaceInside, ALIGN_BUTTONS, MATERIAL_BUTTONS, SPACE_BUTTONS } from './model';

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('align form model', () => {
  it('lists the Align to Selection buttons, top and bottom outside', () => {
    expect(ALIGN_BUTTONS.map((b) => [b.mode, b.label])).toEqual([
      ['centre', 'Align Center'],
      ['vCentre', 'Align Vertical Center'],
      ['hCentre', 'Align Horizontal Center'],
      ['left', 'Align Left'],
      ['right', 'Align Right'],
      ['outsideTop', 'Align Outside Top'],
      ['outsideBottom', 'Align Outside Bottom'],
    ]);
  });

  it('spaces inside the last object and in selection order', () => {
    const items = [
      { id: 1, bbox: [0, 0, 10, 10] as [number, number, number, number] },
      { id: 2, bbox: [12, 0, 22, 10] as [number, number, number, number] },
    ];
    const inside = spaceInside(items, [0, 0, 50, 10], true);
    // 30 spare over three gaps of 10.
    expect(inside.map((m) => m.matrix[4])).toEqual([10, 18]);
    const ordered = spaceInOrder(
      [
        { id: 3, bbox: [0, 0, 2, 2] },
        { id: 4, bbox: [90, 0, 92, 2] },
        { id: 5, bbox: [20, 0, 22, 2] },
      ],
      true,
    );
    // Picked order 3, 4, 5: 5 is last and stays, 4 goes to the middle.
    expect(ordered.map((m) => m.matrix[4])).toEqual([0, -80, 0]);
  });

  it('gives every button an icon, the Align Objects group ones where the group has the mode', () => {
    const buttons = [...MATERIAL_BUTTONS, ...ALIGN_BUTTONS, ...SPACE_BUTTONS];
    expect(buttons.filter((b) => toolIcon(b.icon) === null)).toEqual([]);
    const groupIcons = new Set(
      TOOL_GROUPS.design.flatMap((g) => (g.id === 'alignObjects' ? g.tools : [])).map((t) => t.id),
    );
    expect([...MATERIAL_BUTTONS, ...ALIGN_BUTTONS].filter((b) => !groupIcons.has(b.icon))).toEqual(
      [],
    );
  });
});
