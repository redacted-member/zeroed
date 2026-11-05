import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';

import { strings } from '../../lib/strings';
import { useGadgetStore } from '../../state/gadgetStore';
import { toolIcon, useExtraIcons } from '../ToolIcons/ToolIcons';
import { buildMenus, type MenuEntry, type MenuId, type MenuItem } from './model';
import { runMenuAction, useMenuContext } from './useMenuContext';

import './MenuBar.scss';

type Focus = { list: string; target: 'first' | 'last' } | { title: MenuId } | null;

function enabledButtons(list: HTMLElement | null): HTMLButtonElement[] {
  if (list === null) {
    return [];
  }
  return [...list.querySelectorAll<HTMLButtonElement>(':scope > li > button:not(:disabled)')];
}

/** The check or radio mark of a set item, drawn left of its icon. */
function mark(item: { checked?: boolean; radio?: boolean }): string | null {
  if (item.checked !== true) {
    return null;
  }
  return item.radio === true ? '•' : '✓';
}

/**
 * The gutter left of a label: the item's icon, after a column for the check
 * mark when the list holds check or radio items, so every label of a list
 * starts at the same place.
 */
function gutter(
  item: { icon?: string; checked?: boolean; radio?: boolean },
  marks: boolean,
): JSX.Element {
  return (
    <>
      {marks ? (
        <span aria-hidden="true" className="menu-bar__check">
          {mark(item)}
        </span>
      ) : null}
      <span aria-hidden="true" className="menu-bar__mark">
        {item.icon === undefined ? null : toolIcon(item.icon)}
      </span>
    </>
  );
}

/** Whether a list holds check or radio items. */
function hasMarks(entries: readonly MenuEntry[]): boolean {
  return entries.some((entry) => entry.kind === 'item' && entry.checked !== undefined);
}

function itemRole(item: MenuItem): 'menuitem' | 'menuitemcheckbox' | 'menuitemradio' {
  if (item.checked === undefined) {
    return 'menuitem';
  }
  return item.radio === true ? 'menuitemradio' : 'menuitemcheckbox';
}

/**
 * The menu bar across the top of the window when the app runs in a browser.
 * The desktop build has the same menus in the system menu bar instead.
 */
export function MenuBar(): JSX.Element {
  // Menu, form and panel icons, and the icons menu commands name, load after
  // start up; the render once they are in builds the menus again.
  useExtraIcons();
  const ctx = useMenuContext();
  const menus = useMemo(() => buildMenus(ctx), [ctx]);
  const loaded = useGadgetStore((s) => s.loaded);
  const load = useGadgetStore((s) => s.load);
  const [open, setOpen] = useState<MenuId | null>(null);
  const [sub, setSub] = useState<string | null>(null);
  const [focus, setFocus] = useState<Focus>(null);
  const barRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!loaded) {
      void load();
    }
  }, [loaded, load]);

  // Close on a click anywhere else.
  useEffect(() => {
    if (open === null) {
      return undefined;
    }
    const onPointerDown = (event: PointerEvent) => {
      if (barRef.current !== null && !barRef.current.contains(event.target as Node)) {
        setOpen(null);
        setSub(null);
      }
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    return () => window.removeEventListener('pointerdown', onPointerDown, true);
  }, [open]);

  // Move keyboard focus once the list it targets is on screen.
  useEffect(() => {
    if (focus === null || barRef.current === null) {
      return;
    }
    if ('title' in focus) {
      barRef.current.querySelector<HTMLButtonElement>(`#menu-title-${focus.title}`)?.focus();
    } else {
      const list = barRef.current.querySelector<HTMLElement>(`[data-list="${focus.list}"]`);
      const buttons = enabledButtons(list);
      (focus.target === 'first' ? buttons[0] : buttons[buttons.length - 1])?.focus();
    }
    setFocus(null);
  }, [focus]);

  const ids = menus.map((menu) => menu.id);
  const neighbour = (id: MenuId, step: number): MenuId =>
    ids[(ids.indexOf(id) + step + ids.length) % ids.length] ?? id;

  const openMenu = (id: MenuId, withFocus: boolean) => {
    setOpen(id);
    setSub(null);
    if (withFocus) {
      setFocus({ list: id, target: 'first' });
    }
  };

  const close = (returnTo: MenuId | null) => {
    setOpen(null);
    setSub(null);
    if (returnTo !== null) {
      setFocus({ title: returnTo });
    }
  };

  const choose = (item: MenuItem) => {
    close(null);
    void runMenuAction(item.action);
  };

  const onTitleKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, id: MenuId) => {
    switch (event.key) {
      case 'ArrowDown':
      case 'Enter':
      case ' ':
        event.preventDefault();
        openMenu(id, true);
        break;
      case 'ArrowUp':
        event.preventDefault();
        setOpen(id);
        setSub(null);
        setFocus({ list: id, target: 'last' });
        break;
      case 'ArrowRight':
      case 'ArrowLeft': {
        event.preventDefault();
        const next = neighbour(id, event.key === 'ArrowRight' ? 1 : -1);
        if (open !== null) {
          openMenu(next, true);
        } else {
          setFocus({ title: next });
        }
        break;
      }
      case 'Escape':
        close(id);
        break;
      default:
        break;
    }
  };

  const onListKeyDown = (
    event: ReactKeyboardEvent<HTMLUListElement>,
    menu: MenuId,
    parent: string | null,
  ) => {
    const list = event.currentTarget;
    const buttons = enabledButtons(list);
    const index = buttons.findIndex((b) => b === document.activeElement);
    const current = buttons[index];
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        buttons[(index + 1) % buttons.length]?.focus();
        break;
      case 'ArrowUp':
        event.preventDefault();
        buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
        break;
      case 'Home':
        event.preventDefault();
        buttons[0]?.focus();
        break;
      case 'End':
        event.preventDefault();
        buttons[buttons.length - 1]?.focus();
        break;
      case 'ArrowRight': {
        event.preventDefault();
        const submenu = current?.dataset.submenu;
        if (submenu !== undefined) {
          setSub(submenu);
          setFocus({ list: submenu, target: 'first' });
        } else {
          openMenu(neighbour(menu, 1), true);
        }
        break;
      }
      case 'ArrowLeft':
        event.preventDefault();
        if (parent !== null) {
          setSub(null);
          barRef.current?.querySelector<HTMLButtonElement>(`[data-submenu="${parent}"]`)?.focus();
        } else {
          openMenu(neighbour(menu, -1), true);
        }
        break;
      case 'Escape':
        event.preventDefault();
        if (parent !== null) {
          setSub(null);
          barRef.current?.querySelector<HTMLButtonElement>(`[data-submenu="${parent}"]`)?.focus();
        } else {
          close(menu);
        }
        break;
      case 'Tab':
        close(null);
        break;
      default:
        return;
    }
    event.stopPropagation();
  };

  const renderList = (
    menu: MenuId,
    listId: string,
    label: string,
    entries: readonly MenuEntry[],
    parent: string | null,
    marks = hasMarks(entries),
  ): JSX.Element => (
    <ul
      aria-label={label}
      className={parent === null ? 'menu-bar__list' : 'menu-bar__list menu-bar__list--sub'}
      data-list={listId}
      onKeyDown={(event) => onListKeyDown(event, menu, parent)}
      role="menu"
    >
      {entries.map((entry, index) => {
        if (entry.kind === 'separator') {
          return <li className="menu-bar__separator" key={`sep-${index}`} role="separator" />;
        }
        if (entry.kind === 'submenu') {
          const expanded = sub === entry.id;
          return (
            <li className="menu-bar__entry" key={entry.id} role="none">
              <button
                aria-expanded={expanded}
                aria-haspopup="menu"
                className="menu-bar__item"
                data-submenu={entry.id}
                onClick={() => setSub(expanded ? null : entry.id)}
                onPointerEnter={() => setSub(entry.id)}
                role="menuitem"
                tabIndex={-1}
                type="button"
              >
                {gutter(entry, marks)}
                <span className="menu-bar__label">{entry.label}</span>
                <span aria-hidden="true" className="menu-bar__arrow">
                  {'›'}
                </span>
              </button>
              {expanded ? renderList(menu, entry.id, entry.label, entry.entries, entry.id) : null}
            </li>
          );
        }
        return (
          <li className="menu-bar__entry" key={entry.id} role="none">
            <button
              aria-checked={entry.checked}
              className="menu-bar__item"
              disabled={entry.disabled}
              onClick={() => choose(entry)}
              onPointerEnter={() => {
                if (parent === null) {
                  setSub(null);
                }
              }}
              role={itemRole(entry)}
              tabIndex={-1}
              data-tip={entry.hint}
              type="button"
            >
              {gutter(entry, marks)}
              <span className="menu-bar__label">
                {entry.label}
                {entry.note === undefined ? null : (
                  <span className="menu-bar__note">{entry.note}</span>
                )}
              </span>
              {entry.shortcut === undefined ? null : (
                <span className="menu-bar__shortcut">{entry.shortcut}</span>
              )}
            </button>
          </li>
        );
      })}
    </ul>
  );

  return (
    <nav aria-label={strings.menuBar.label} className="menu-bar" ref={barRef}>
      <ul className="menu-bar__bar" role="menubar">
        {menus.map((menu, index) => {
          const isOpen = open === menu.id;
          return (
            <li className="menu-bar__menu" key={menu.id} role="none">
              <button
                aria-expanded={isOpen}
                aria-haspopup="menu"
                className="menu-bar__title"
                id={`menu-title-${menu.id}`}
                onClick={() => (isOpen ? close(null) : openMenu(menu.id, false))}
                onKeyDown={(event) => onTitleKeyDown(event, menu.id)}
                onPointerEnter={() => {
                  if (open !== null && !isOpen) {
                    openMenu(menu.id, false);
                  }
                }}
                role="menuitem"
                tabIndex={index === 0 ? 0 : -1}
                type="button"
              >
                {menu.label}
              </button>
              {isOpen ? renderList(menu.id, menu.id, menu.label, menu.entries, null) : null}
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
