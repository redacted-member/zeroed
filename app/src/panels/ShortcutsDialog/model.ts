// The Keyboard Shortcuts list, generated from the key table in
// lib/shortcuts so it never drifts from the keys that work. Pure.

import { comboLabel, keys } from '../../lib/keyLabels';
import { dialogsStrings } from '../../lib/strings/dialogs';
import { SHORTCUTS, type AppCommand, type Shortcut } from '../../lib/shortcuts';

const text = dialogsStrings.shortcutsDialog;

export type ShortcutSectionId = 'file' | 'edit' | 'view' | 'panels' | 'nudge';

export interface ShortcutRow {
  /** The command, or a nudge id for the arrow key rows. */
  id: string;
  label: string;
  /** Every key bound to the command, formatted for the platform. */
  keys: string[];
}

export interface ShortcutSection {
  id: ShortcutSectionId;
  title: string;
  rows: ShortcutRow[];
}

const KEY_NAMES: Readonly<Record<string, string>> = {
  Delete: 'Del',
  Backspace: 'Backspace',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
};

/** One key combination as the platform writes it. */
export function keyLabel(shortcut: Shortcut, mac: boolean): string {
  return comboLabel(shortcut, mac, KEY_NAMES);
}

/** Which section a command belongs to. */
export function sectionOf(command: AppCommand): ShortcutSectionId {
  if (command.startsWith('file.')) {
    return 'file';
  }
  if (command.startsWith('edit.') || command === 'app.preferences') {
    return 'edit';
  }
  if (command.startsWith('view.') || command === 'help.contents') {
    return 'view';
  }
  return 'panels';
}

/** The words for a command: its own label, or the command id as a fallback. */
export function commandLabel(command: AppCommand): string {
  return text.commands[command] ?? command;
}

const ORDER: readonly ShortcutSectionId[] = ['file', 'edit', 'view', 'panels'];

/** The shortcut sections, each command once with all its keys, in table order. */
export function shortcutSections(mac: boolean): ShortcutSection[] {
  const rows = new Map<AppCommand, ShortcutRow>();
  for (const shortcut of SHORTCUTS) {
    const row = rows.get(shortcut.command);
    const key = keyLabel(shortcut, mac);
    if (row === undefined) {
      rows.set(shortcut.command, {
        id: shortcut.command,
        label: commandLabel(shortcut.command),
        keys: [key],
      });
    } else if (!row.keys.includes(key)) {
      row.keys.push(key);
    }
  }
  const sections: ShortcutSection[] = ORDER.map((id) => ({
    id,
    title: text.sections[id],
    rows: [...rows.entries()]
      .filter(([command]) => sectionOf(command) === id)
      .map(([, row]) => row),
  })).filter((section) => section.rows.length > 0);
  const n = text.nudge;
  sections.push({
    id: 'nudge',
    title: text.sections.nudge,
    rows: [
      { id: 'nudge.one', label: n.one, keys: [n.arrows] },
      { id: 'nudge.ten', label: n.ten, keys: [n.withArrows(keys('Shift', mac))] },
      { id: 'nudge.tenth', label: n.tenth, keys: [n.withArrows(keys('Ctrl', mac))] },
      { id: 'nudge.fixed', label: n.fixed, keys: [n.withArrows(keys('Ctrl+Shift', mac))] },
    ],
  });
  return sections;
}
