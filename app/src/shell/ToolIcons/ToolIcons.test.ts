import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import { EXTRA_ICONS } from './extraIcons';
import { drawIcon, loadExtraIcons, toolIcon } from './ToolIcons';

describe('toolIcon', () => {
  it('has an icon for every tool that opens a form or runs a command', () => {
    const missing = Object.values(TOOL_GROUPS)
      .flat()
      .flatMap((group) => group.tools)
      .filter((tool) => tool.form !== undefined || tool.command !== undefined)
      .filter((tool) => toolIcon(tool.id) === null)
      .map((tool) => tool.id);
    expect(missing).toEqual([]);
  });

  it('returns null for unknown and placeholder ids', () => {
    expect(toolIcon('noSuchTool')).toBeNull();
    expect(toolIcon('toString')).toBeNull();
    expect(toolIcon('file-1')).toBeNull();
  });

  it('draws the menu and form icons only once their lazy set is in, none shadowed', async () => {
    expect(toolIcon('editCut')).toBeNull();
    await loadExtraIcons();
    expect(toolIcon('editCut')).not.toBeNull();
    // No lazy icon shares an id with a start up one, which would hide it.
    const markup = (id: string) => {
      const icon = toolIcon(id);
      return icon === null ? '' : renderToStaticMarkup(icon);
    };
    const shadowed = [...EXTRA_ICONS].filter(
      ([id, spec]) => markup(id) !== renderToStaticMarkup(drawIcon(spec)),
    );
    expect(shadowed).toEqual([]);
  });

  it('draws the icons the popups and the Layers and Sheets buttons name', async () => {
    await loadExtraIcons();
    const ids = [
      'gadget',
      'variables',
      'variableNew',
      'segmentDeleteChildren',
      'segmentSeparate',
      'layerAdd',
      'visible',
      'hidden',
      'layerHideEmpty',
      'sheetAdd',
      'sheetEdit',
      'sheetDelete',
    ];
    expect(ids.filter((id) => toolIcon(id) === null)).toEqual([]);
  });
});
