// The list is the ARIA combobox pattern: focus stays on the button and
// aria-activedescendant names the highlighted option, so the listbox itself
// is never focused.
/* eslint-disable jsx-a11y/click-events-have-key-events */
import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type JSX,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
} from 'react';
import { createPortal } from 'react-dom';

import {
  MIN_LIST_HEIGHT,
  USUAL_ROWS,
  clampListSize,
  draggedSize,
  keyedIndex,
  typeAhead,
  type ListSize,
} from './model';

import './ResizableSelect.scss';

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectGroup {
  /** Heading shown above the group; none for a plain list. */
  label?: string;
  options: readonly SelectOption[];
}

interface ResizableSelectProps {
  id: string;
  value: string;
  groups: readonly SelectGroup[];
  onChange: (value: string) => void;
  /** The size the open list was dragged to, or null for the usual size. */
  size: ListSize | null;
  /** Called when a drag ends (the new size) or the corner is double clicked (null). */
  onResize: (size: ListSize | null) => void;
  /** Tooltip and label of the corner grip. */
  resizeLabel: string;
  /** Shown while there are no options. */
  placeholder?: string;
  disabled?: boolean;
  className?: string;
}

/** Height of one option row in CSS pixels, matching the Sass. */
const ROW_PX = 22;
/** How long type-ahead letters build one query, in ms. */
const TYPE_AHEAD_MS = 700;

interface Placement {
  left: number;
  top: number;
  /** The button's width, the narrowest the list gets. */
  minWidth: number;
  /** Room below the button. */
  room: number;
}

/**
 * A drop-down list whose open list can be dragged bigger or smaller by its
 * bottom right corner to show more items, and remembers that size.
 */
export function ResizableSelect({
  id,
  value,
  groups,
  onChange,
  size,
  onResize,
  resizeLabel,
  placeholder,
  disabled,
  className,
}: ResizableSelectProps): JSX.Element {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const [place, setPlace] = useState<Placement | null>(null);
  const [dragSize, setDragSize] = useState<ListSize | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; size: ListSize } | null>(null);
  const typed = useRef<{ query: string; at: number }>({ query: '', at: 0 });

  const flat = groups.flatMap((group) => group.options);
  const labels = flat.map((option) => option.label);
  // The first match, so a value listed twice (recent and all) highlights once.
  const selected = flat.findIndex((option) => option.value === value);
  const listId = `${id}-list`;
  const optionId = (index: number) => `${id}-option-${index}`;
  const current = selected < 0 ? undefined : flat[selected];

  const close = () => {
    setOpen(false);
    setDragSize(null);
    drag.current = null;
  };

  const openList = () => {
    const box = button.current?.getBoundingClientRect();
    if (box === undefined || flat.length === 0) {
      return;
    }
    setPlace({
      left: box.left,
      top: box.bottom,
      minWidth: box.width,
      room: Math.max(MIN_LIST_HEIGHT, window.innerHeight - box.bottom - 8),
    });
    setActive(selected < 0 ? 0 : selected);
    setOpen(true);
  };

  const choose = (index: number) => {
    const option = flat[index];
    if (option !== undefined) {
      onChange(option.value);
    }
    close();
    button.current?.focus();
  };

  useEffect(() => {
    if (!open) {
      return;
    }
    const inside = (target: EventTarget | null) =>
      target instanceof Node &&
      (button.current?.contains(target) === true || list.current?.contains(target) === true);
    const onPointerDown = (event: globalThis.PointerEvent) => {
      if (!inside(event.target)) {
        close();
      }
    };
    const onScroll = (event: Event) => {
      if (!inside(event.target)) {
        close();
      }
    };
    const onResizeWindow = () => close();
    document.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onResizeWindow);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', onResizeWindow);
    };
  }, [open]);

  useEffect(() => {
    if (open && active >= 0) {
      list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
    }
  }, [open, active]);

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
        event.preventDefault();
        openList();
      }
      return;
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (event.key === 'Enter' || (event.key === ' ' && typed.current.query === '')) {
      event.preventDefault();
      choose(active);
      return;
    }
    if (event.key === 'Tab') {
      close();
      return;
    }
    const next = keyedIndex(active, flat.length, event.key);
    if (next !== null) {
      event.preventDefault();
      setActive(next);
      return;
    }
    if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      const now = event.timeStamp;
      const query =
        now - typed.current.at > TYPE_AHEAD_MS ? event.key : typed.current.query + event.key;
      typed.current = { query, at: now };
      const found = typeAhead(labels, query, active);
      if (found >= 0) {
        setActive(found);
      }
    }
  };

  const indexOf = (event: MouseEvent<HTMLElement>): number => {
    const option = (event.target as HTMLElement).closest('[data-index]');
    return option === null ? -1 : Number(option.getAttribute('data-index'));
  };

  const onGripDown = (event: PointerEvent<HTMLDivElement>) => {
    const box = list.current?.getBoundingClientRect();
    if (event.button !== 0 || box === undefined) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      x: event.clientX,
      y: event.clientY,
      size: { width: box.width, height: box.height },
    };
  };
  const onGripMove = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (start !== null) {
      setDragSize(draggedSize(start.size, event.clientX - start.x, event.clientY - start.y));
    }
  };
  const onGripUp = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (drag.current !== null && dragSize !== null) {
      onResize(dragSize);
    }
    drag.current = null;
    setDragSize(null);
  };

  const shown = dragSize ?? clampListSize(size);
  const listStyle =
    place === null
      ? undefined
      : {
          left: place.left,
          top: place.top,
          width: Math.max(place.minWidth, shown?.width ?? 0),
          height: shown?.height ?? Math.min(place.room, USUAL_ROWS * ROW_PX + 2),
        };

  // Where each group's options start in the flat list.
  const starts = groups.map((_, g) =>
    groups.slice(0, g).reduce((sum, group) => sum + group.options.length, 0),
  );
  const option = (item: SelectOption, at: number) => {
    return (
      <div
        aria-selected={at === selected}
        className={
          at === active
            ? 'resizable-select__option resizable-select__option--active'
            : 'resizable-select__option'
        }
        data-index={at}
        id={optionId(at)}
        key={optionId(at)}
        role="option"
      >
        {item.label}
      </div>
    );
  };

  return (
    <>
      <button
        aria-activedescendant={open && active >= 0 ? optionId(active) : undefined}
        aria-controls={listId}
        aria-expanded={open}
        aria-haspopup="listbox"
        className={className === undefined ? 'resizable-select' : `resizable-select ${className}`}
        disabled={disabled}
        id={id}
        onClick={() => (open ? close() : openList())}
        onKeyDown={onKeyDown}
        ref={button}
        role="combobox"
        type="button"
      >
        <span className="resizable-select__value">{current?.label ?? placeholder ?? ''}</span>
        <span aria-hidden="true" className="resizable-select__caret" />
      </button>
      {open
        ? createPortal(
            <div
              className="resizable-select__list"
              id={listId}
              onClick={(event) => {
                const at = indexOf(event);
                if (at >= 0) {
                  choose(at);
                }
              }}
              onMouseMove={(event) => {
                const at = indexOf(event);
                if (at >= 0 && at !== active) {
                  setActive(at);
                }
              }}
              ref={list}
              role="listbox"
              style={listStyle}
              tabIndex={-1}
            >
              <div className="resizable-select__scroll">
                {groups.map((group, g) =>
                  group.label === undefined ? (
                    <Fragment key={g}>
                      {group.options.map((item, k) => option(item, starts[g] + k))}
                    </Fragment>
                  ) : (
                    <div aria-labelledby={`${id}-group-${g}`} key={g} role="group">
                      <div
                        className="resizable-select__group"
                        id={`${id}-group-${g}`}
                        role="presentation"
                      >
                        {group.label}
                      </div>
                      {group.options.map((item, k) => option(item, starts[g] + k))}
                    </div>
                  ),
                )}
              </div>
              <div
                aria-hidden="true"
                className="resizable-select__grip"
                onDoubleClick={(event) => {
                  event.stopPropagation();
                  onResize(null);
                }}
                onLostPointerCapture={() => (drag.current = null)}
                onPointerDown={onGripDown}
                onPointerMove={onGripMove}
                onPointerUp={onGripUp}
                data-tip={resizeLabel}
              />
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
