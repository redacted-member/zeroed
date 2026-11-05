import { useEffect, useRef, useState, type JSX, type KeyboardEvent } from 'react';

import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import {
  isSeparator,
  isSubmenu,
  menuKey,
  submenuDisabled,
  type PopupEntry,
  type PopupItem,
  type PopupSubmenu,
} from './model';

import './PopupMenu.scss';

interface PopupMenuProps<A> {
  /** Client coordinates of the pointer that opened the menu. */
  x: number;
  y: number;
  label: string;
  entries: readonly PopupEntry<A>[];
  onSelect: (action: A) => void;
  onClose: () => void;
}

const WIDTH = 240;

/** The gutter left of a label: the item's icon, after a column for the check mark
 * when the menu holds checked items, so the labels of a menu stay aligned. */
function Gutter({
  icon,
  checked,
  marks,
}: {
  icon?: string;
  checked?: boolean;
  marks: boolean;
}): JSX.Element {
  return (
    <>
      {marks ? (
        <span aria-hidden="true" className="popup-menu__check">
          {checked === true ? '✓' : null}
        </span>
      ) : null}
      <span aria-hidden="true" className="popup-menu__gutter">
        {icon === undefined ? null : toolIcon(icon)}
      </span>
    </>
  );
}
const ITEM_HEIGHT = 26;

/** The enabled buttons of one menu level, not those of its submenus. */
function levelButtons(list: HTMLUListElement | null): HTMLButtonElement[] {
  return [
    ...(list?.querySelectorAll<HTMLButtonElement>(':scope > li > button:not(:disabled)') ?? []),
  ];
}

interface MenuListProps<A> {
  entries: readonly PopupEntry<A>[];
  label: string;
  /** A submenu, not the top menu. */
  nested: boolean;
  /** Put the focus on the first item when shown. */
  focusFirst: boolean;
  onPick: (action: A) => void;
  /** Escape (or Left in a submenu): closes this level. */
  onCloseLevel: () => void;
  className: string;
  style?: { left: string; top: string };
  listRef?: (element: HTMLUListElement | null) => void;
}

function MenuList<A>({
  entries,
  label,
  nested,
  focusFirst,
  onPick,
  onCloseLevel,
  className,
  style,
  listRef,
}: MenuListProps<A>): JSX.Element {
  const ref = useRef<HTMLUListElement | null>(null);
  const marks = entries.some(
    (entry) => !isSeparator(entry) && 'checked' in entry && entry.checked !== undefined,
  );
  const [open, setOpen] = useState<{ id: string; byKey: boolean } | null>(null);

  useEffect(() => {
    if (focusFirst) {
      levelButtons(ref.current)[0]?.focus();
    }
  }, [focusFirst]);

  const closeSubmenu = (id: string) => {
    setOpen(null);
    ref.current?.querySelector<HTMLButtonElement>(`[data-submenu="${id}"]`)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    // Each level handles its own keys; the level above never sees them.
    event.stopPropagation();
    const buttons = levelButtons(ref.current);
    const focused = document.activeElement;
    const index = buttons.findIndex((b) => b === focused);
    const current = buttons[index];
    const submenuId = current?.dataset['submenu'];
    const move = menuKey(event.key, index, buttons.length, submenuId !== undefined, nested);
    if (move.kind === 'none') {
      return;
    }
    event.preventDefault();
    if (move.kind === 'focus') {
      setOpen(null);
      buttons[move.index]?.focus();
    } else if (move.kind === 'open' && submenuId !== undefined) {
      setOpen({ id: submenuId, byKey: true });
    } else if (move.kind === 'close') {
      onCloseLevel();
    }
  };

  const item = (entry: PopupItem<A>) => (
    <li className="popup-menu__item" key={entry.id} role="none">
      <button
        aria-checked={entry.checked}
        className="popup-menu__button"
        disabled={entry.disabled}
        onClick={() => onPick(entry.action)}
        onPointerEnter={() => setOpen(null)}
        role={entry.checked === undefined ? 'menuitem' : 'menuitemradio'}
        type="button"
      >
        <Gutter checked={entry.checked} icon={entry.icon} marks={marks} />
        <span className="popup-menu__label">{entry.label}</span>
        {entry.shortcut === undefined ? null : (
          <span className="popup-menu__shortcut">{entry.shortcut}</span>
        )}
      </button>
    </li>
  );

  const submenu = (entry: PopupSubmenu<A>) => {
    const shown = open?.id === entry.id;
    return (
      <li className="popup-menu__item popup-menu__item--parent" key={entry.id} role="none">
        <button
          aria-expanded={shown}
          aria-haspopup="menu"
          className="popup-menu__button"
          data-submenu={entry.id}
          disabled={submenuDisabled(entry)}
          onClick={() => setOpen(shown ? null : { id: entry.id, byKey: false })}
          onPointerEnter={() => setOpen({ id: entry.id, byKey: false })}
          role="menuitem"
          type="button"
        >
          <Gutter icon={entry.icon} marks={marks} />
          <span className="popup-menu__label">{entry.label}</span>
          <span aria-hidden="true" className="popup-menu__arrow">
            ▸
          </span>
        </button>
        {shown ? (
          <MenuList
            className="popup-menu popup-menu--sub"
            entries={entry.entries}
            focusFirst={open.byKey}
            label={entry.label}
            nested
            onCloseLevel={() => closeSubmenu(entry.id)}
            onPick={onPick}
          />
        ) : null}
      </li>
    );
  };

  return (
    <ul
      aria-label={label}
      className={className}
      onKeyDown={onKeyDown}
      ref={(element) => {
        ref.current = element;
        listRef?.(element);
      }}
      role="menu"
      style={style}
    >
      {entries.map((entry, index) => {
        if (isSeparator(entry)) {
          return <li aria-hidden="true" className="popup-menu__separator" key={`sep-${index}`} />;
        }
        return isSubmenu(entry) ? submenu(entry) : item(entry);
      })}
    </ul>
  );
}

/** A right click menu at the pointer, with submenus opening beside their
 * items. Closes on Escape, outside clicks or a choice. Keys: Up and Down
 * move, Right (or Enter) opens a submenu, Left or Escape closes it. */
export function PopupMenu<A>({
  x,
  y,
  label,
  entries,
  onSelect,
  onClose,
}: PopupMenuProps<A>): JSX.Element {
  // Menu, form and panel icons load after start up; redraw once they are in.
  useExtraIcons();
  const listRef = useRef<HTMLUListElement | null>(null);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (listRef.current !== null && !listRef.current.contains(event.target as Node)) {
        onClose();
      }
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    return () => window.removeEventListener('pointerdown', onPointerDown, true);
  }, [onClose]);

  // Keep the menu inside the window.
  const left = Math.min(x, window.innerWidth - WIDTH - 8);
  const top = Math.min(y, window.innerHeight - entries.length * ITEM_HEIGHT - 16);

  return (
    <MenuList
      className="popup-menu"
      entries={entries}
      focusFirst
      label={label}
      listRef={(element) => {
        listRef.current = element;
      }}
      nested={false}
      onCloseLevel={onClose}
      onPick={(action) => {
        onClose();
        onSelect(action);
      }}
      style={{ left: `${Math.max(0, left)}px`, top: `${Math.max(0, top)}px` }}
    />
  );
}
