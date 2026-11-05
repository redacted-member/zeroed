// The tooltip of a tool grid icon: the tool's name and the key that runs
// the same tool, if any, then a line saying what the tool does.

import { TRANSFORM_FORM_KEYS, type AppCommand } from '../../lib/shortcuts';
import type { FormId } from '../../state/uiStore';
import { strings } from '../../lib/strings';
import type { ToolGroup, ToolSlot } from '../LeftPanel/toolGroups';
import { commandTip } from '../Tooltip/model';

/** The command whose key opens a form (M opens Move, T Set Size and so on). */
const FORM_KEYS = new Map<FormId, AppCommand>(
  Object.entries(TRANSFORM_FORM_KEYS).map(([command, form]) => [form, command as AppCommand]),
);

/** Tools whose key is bound to a sibling command (N toggles node editing). */
const KEY_ALIASES: Partial<Record<AppCommand, AppCommand>> = {
  'tool.node': 'tool.nodeToggle',
};

/** What each tool does, by tool id. */
export type ToolDescriptions = Readonly<Record<string, string>>;

/** Loads the tools' one line descriptions, which stay out of the start bundle. */
export async function loadToolDescriptions(): Promise<ToolDescriptions> {
  return (await import('../../lib/strings/toolDescriptions')).toolDescriptions;
}

/** The tooltip text of a tool grid icon, with its description when loaded. */
export function toolTip(tool: ToolSlot, descriptions: ToolDescriptions = {}): string {
  const command = tool.command ?? (tool.form === undefined ? undefined : FORM_KEYS.get(tool.form));
  return commandTip(
    tool.label,
    command === undefined ? undefined : (KEY_ALIASES[command] ?? command),
    descriptions[tool.id],
  );
}

/** One row of a group's grid: the tools and the caption over them, if any. */
export interface ToolRow {
  caption: string | undefined;
  tools: readonly ToolSlot[];
}

/**
 * A group's tools split into the rows the grid draws. A caption shows only
 * when the group has more than one row, so a lone row is named by its group.
 */
export function toolRows(group: ToolGroup): ToolRow[] {
  const rows: ToolSlot[][] = [];
  group.tools.forEach((tool, index) => {
    if (index === 0 || tool.rowStart === true) {
      rows.push([]);
    }
    rows[rows.length - 1]?.push(tool);
  });
  const captions: Readonly<Record<string, string>> = strings.leftPanel.rows;
  return rows.map((tools) => ({
    caption: rows.length > 1 && tools[0] !== undefined ? captions[tools[0].id] : undefined,
    tools,
  }));
}
