// Entry types shared by every right click menu. Menu builders stay pure and
// return these; PopupMenu renders them.

export interface PopupItem<A> {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  /** Radio style item: shows a mark when true. */
  checked?: boolean;
  /** The shell/ToolIcons id drawn in the gutter left of the label. */
  icon?: string;
  /** A disabled line that only tells something (no variables yet); it has no icon. */
  info?: boolean;
  action: A;
}

/** An item that opens a menu of its own beside it. */
export interface PopupSubmenu<A> {
  id: string;
  label: string;
  entries: readonly PopupEntry<A>[];
  /** The shell/ToolIcons id drawn in the gutter left of the label. */
  icon?: string;
}

export type PopupEntry<A> = PopupItem<A> | PopupSubmenu<A> | { separator: true };

export function isSeparator<A>(entry: PopupEntry<A>): entry is { separator: true } {
  return 'separator' in entry;
}

export function isSubmenu<A>(entry: PopupEntry<A>): entry is PopupSubmenu<A> {
  return 'entries' in entry;
}

/**
 * Gives the items and submenus of a menu the icons `icons` names for their
 * ids, submenus included. An icon an entry names itself stays.
 */
export function withIcons<A>(
  entries: readonly PopupEntry<A>[],
  icons: Readonly<Record<string, string>>,
): PopupEntry<A>[] {
  return entries.map((entry) => {
    if (isSeparator(entry)) {
      return entry;
    }
    const icon = entry.icon ?? icons[entry.id];
    if (isSubmenu(entry)) {
      return { ...entry, icon, entries: withIcons(entry.entries, icons) };
    }
    return icon === undefined ? entry : { ...entry, icon };
  });
}

/** Every item of a menu, those of its submenus included, separators
 * dropped. Handy in tests. */
export function menuItems<A>(entries: readonly PopupEntry<A>[]): PopupItem<A>[] {
  return entries.flatMap((e): PopupItem<A>[] => {
    if (isSeparator(e)) {
      return [];
    }
    return isSubmenu(e) ? menuItems(e.entries) : [e];
  });
}

/** The ids of the items and submenus that draw no icon, lines that only
 * tell something aside. Every other entry has one; tests check it. */
export function entriesWithoutIcon<A>(entries: readonly PopupEntry<A>[]): string[] {
  return entries.flatMap((e): string[] => {
    if (isSeparator(e)) {
      return [];
    }
    const own = e.icon === undefined && !('info' in e && e.info === true) ? [e.id] : [];
    return isSubmenu(e) ? [...own, ...entriesWithoutIcon(e.entries)] : own;
  });
}

/** A submenu is greyed when nothing in it can be picked. */
export function submenuDisabled<A>(submenu: PopupSubmenu<A>): boolean {
  return menuItems(submenu.entries).every((item) => item.disabled === true);
}

/** What a key does in a menu: move the focus among the items of the menu
 * it is in, open the focused submenu, or close the menu it is in (a
 * submenu goes back to its item; the top menu closes). */
export type MenuKeyMove =
  { kind: 'focus'; index: number } | { kind: 'open' } | { kind: 'close' } | { kind: 'none' };

/**
 * The move for `key` with the focus on item `index` of `count` enabled
 * items. `onSubmenu` is true when the focused item opens a submenu and
 * `nested` when the menu is itself a submenu: Right (or Enter and Space)
 * opens a submenu, Left closes a submenu, Escape closes either.
 */
export function menuKey(
  key: string,
  index: number,
  count: number,
  onSubmenu: boolean,
  nested: boolean,
): MenuKeyMove {
  if (key === 'Escape') {
    return { kind: 'close' };
  }
  if (count === 0) {
    return { kind: 'none' };
  }
  switch (key) {
    case 'ArrowDown':
      return { kind: 'focus', index: (index + 1) % count };
    case 'ArrowUp':
      return { kind: 'focus', index: index < 0 ? count - 1 : (index - 1 + count) % count };
    case 'Home':
      return { kind: 'focus', index: 0 };
    case 'End':
      return { kind: 'focus', index: count - 1 };
    case 'ArrowRight':
    case 'Enter':
    case ' ':
      return onSubmenu ? { kind: 'open' } : { kind: 'none' };
    case 'ArrowLeft':
      return nested ? { kind: 'close' } : { kind: 'none' };
    default:
      return { kind: 'none' };
  }
}
