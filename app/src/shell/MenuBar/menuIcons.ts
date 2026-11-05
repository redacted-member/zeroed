// The icon a menu item shows in the gutter left of its label: the icon of
// the tool it runs (shell/ToolIcons), or a common command icon drawn in
// the same style. Every item has one, check and radio items too (their
// mark sits beside the icon). Every menu (the menu bar, the native menu
// and the right click menus) looks its icons up here. The command icon
// table loads with the lazy icon set (ToolIcons/extraIcons.ts); menus are
// built again once it is in.

import type { AppCommand } from '../../lib/shortcuts';
import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import { commandIcon } from '../ToolIcons/ToolIcons';

/** The command a Design tab tool runs, mapped to that tool's icon. */
function toolCommandIcons(): Map<string, string> {
  const icons = new Map<string, string>();
  for (const group of Object.values(TOOL_GROUPS).flat()) {
    for (const tool of group.tools) {
      if (tool.command !== undefined && !icons.has(tool.command)) {
        icons.set(tool.command, tool.id);
      }
    }
  }
  return icons;
}

let byTool: Map<string, string> | null = null;

/** The icon id (a shell/ToolIcons id) for a menu command, or undefined for none. */
export function menuIcon(command: AppCommand): string | undefined {
  const own = commandIcon(command);
  if (own !== undefined) {
    return own;
  }
  byTool ??= toolCommandIcons();
  return byTool.get(command);
}
