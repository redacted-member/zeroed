// Dragging a component row while the Assembly View is open: past a few
// pixels the press becomes a drag and a label follows the pointer. Within
// the tree it reorders, as the tree's own drag does (the row it would go by
// is marked); out over the 3D view a ghost of the piece stands where it
// would land, and letting go lands it there (a component that is a piece
// already moves there). Anywhere else, or Esc, cancels. Pointer events on
// the window, not HTML5 drag and drop, which the desktop shell's file drop
// can swallow.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';

import type { DropLanding } from '../../bridge/assembly';
import { isDrag } from '../ClipartPanel/drop';
import { useAssemblyStore } from '../../state/assemblyStore';
import { pieceDropAt } from '../../views/View3D/pieceDrop';
import type { DropSpot } from './tree';

/** A press on a tree row that may become a drag. */
export interface PiecePress {
  pointerId: number;
  component: number;
  name: string;
  /** What a reorder moves: the picked components when the row is picked. */
  ids: number[];
  from: { x: number; y: number };
  dragging: boolean;
}

/** Where a drag is over the tree: the spot it would reorder to, null where nothing goes. */
export interface TreeOver {
  spot: DropSpot | null;
}

/** What the tree lends the drag: where a screen point is on it, the mark, and the move. */
export interface TreeDrag {
  /** Where a screen point is on the tree, or null off it. */
  at: (x: number, y: number) => TreeOver | null;
  /** Marks the spot a reorder would go to; null clears it. */
  mark: (spot: DropSpot | null) => void;
  /** Moves `ids` to `spot`. */
  reorder: (ids: number[], spot: DropSpot) => void;
}

/**
 * The spot a row stands for, from its data attributes (`dropComponent`,
 * `dropGroup` with `dropLevel`, or `dropLevel` alone) and where the
 * pointer is on it: the upper half of a component row is later in the
 * bottom up list, as the tree's own drag has it.
 */
export function rowSpot(
  data: Readonly<Record<string, string | undefined>>,
  top: number,
  height: number,
  y: number,
): DropSpot | null {
  const number = (value: string | undefined) =>
    value === undefined || value === '' || !Number.isFinite(Number(value)) ? null : Number(value);
  const component = number(data['dropComponent']);
  const group = number(data['dropGroup']);
  const level = number(data['dropLevel']);
  if (component !== null) {
    return { kind: 'component', id: component, after: y < top + height / 2 };
  }
  if (group !== null && level !== null) {
    return { kind: 'group', level, group };
  }
  return level === null ? null : { kind: 'level', level };
}

/** The label that follows the pointer; `over` while a view would take the piece. */
export interface DragLabel {
  name: string;
  x: number;
  y: number;
  over: boolean;
}

/** What a pointer move does to a press: nothing yet, starts the drag, or moves it. */
export type PressStep = 'wait' | 'start' | 'move';

export function pressStep(press: PiecePress, at: { x: number; y: number }): PressStep {
  if (press.dragging) {
    return 'move';
  }
  return isDrag(press.from, at) ? 'start' : 'wait';
}

/** What letting go does. */
export type ReleaseAction =
  | { kind: 'none' }
  | { kind: 'cancel' }
  | { kind: 'reorder'; ids: number[]; spot: DropSpot }
  | { kind: 'drop'; component: number; at: DropLanding };

/**
 * What letting go does: over the tree, reorders to the spot there (cancels
 * where nothing goes); over a view, lands the component there; anywhere
 * else, or before the press became a drag, nothing lands.
 */
export function releaseAction(
  press: PiecePress,
  at: DropLanding | null,
  tree: TreeOver | null = null,
): ReleaseAction {
  if (!press.dragging) {
    return { kind: 'none' };
  }
  if (tree !== null) {
    return tree.spot === null
      ? { kind: 'cancel' }
      : { kind: 'reorder', ids: press.ids, spot: tree.spot };
  }
  return at === null ? { kind: 'cancel' } : { kind: 'drop', component: press.component, at };
}

/** Whether a press starts on something that takes presses of its own (a check box, a text box). */
function ownsPress(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest('input, textarea, select') !== null;
}

/**
 * The drag of tree rows within the tree and onto the 3D view. `active`
 * while the Assembly View is open; `press` starts one from a row's pointer
 * down, `ids` being what a reorder moves.
 */
export function usePieceDrag(tree: TreeDrag): {
  active: boolean;
  label: DragLabel | null;
  press: (
    event: ReactPointerEvent<HTMLElement>,
    component: number,
    name: string,
    ids: number[],
  ) => void;
} {
  const active = useAssemblyStore((s) => s.active);
  const [label, setLabel] = useState<DragLabel | null>(null);
  const stop = useRef<(() => void) | null>(null);

  useEffect(() => () => stop.current?.(), []);
  useEffect(() => {
    if (!active) {
      stop.current?.();
    }
  }, [active]);

  const press = (
    event: ReactPointerEvent<HTMLElement>,
    component: number,
    name: string,
    ids: number[],
  ) => {
    if (!active || event.button !== 0 || ownsPress(event.target) || stop.current !== null) {
      return;
    }
    const held: PiecePress = {
      pointerId: event.pointerId,
      component,
      name,
      ids,
      from: { x: event.clientX, y: event.clientY },
      dragging: false,
    };
    const store = useAssemblyStore.getState;

    const onMove = (move: PointerEvent) => {
      if (move.pointerId !== held.pointerId) {
        return;
      }
      const at = { x: move.clientX, y: move.clientY };
      const step = pressStep(held, at);
      if (step === 'wait') {
        return;
      }
      if (step === 'start') {
        held.dragging = true;
        store().startPlacing(component, true);
      }
      move.preventDefault();
      const over = tree.at(at.x, at.y);
      if (over !== null) {
        // Within the tree: a reorder, no ghost in the view.
        tree.mark(over.spot);
        store().movePlacing(null);
        setLabel({ name, x: at.x, y: at.y, over: over.spot !== null });
        return;
      }
      tree.mark(null);
      const landing = pieceDropAt(at.x, at.y);
      store().movePlacing(landing);
      setLabel({ name, x: at.x, y: at.y, over: landing !== null });
    };

    // The click that ends a drag does not pick a row.
    const swallowClick = (click: MouseEvent) => {
      click.stopPropagation();
      click.preventDefault();
    };

    const onUp = (up: PointerEvent) => {
      if (up.pointerId !== held.pointerId) {
        return;
      }
      const released = up.type === 'pointerup';
      const over = released ? tree.at(up.clientX, up.clientY) : null;
      const action = releaseAction(
        held,
        released && over === null ? pieceDropAt(up.clientX, up.clientY) : null,
        over,
      );
      finish();
      if (action.kind === 'none') {
        return;
      }
      window.addEventListener('click', swallowClick, { capture: true, once: true });
      window.setTimeout(() => window.removeEventListener('click', swallowClick, true), 0);
      if (action.kind === 'drop') {
        void store().dropComponent(action.component, action.at);
        return;
      }
      store().cancelPlacing();
      if (action.kind === 'reorder') {
        tree.reorder(action.ids, action.spot);
      }
    };

    const onKey = (key: KeyboardEvent) => {
      if (key.key === 'Escape' && held.dragging) {
        key.preventDefault();
        finish();
        store().cancelPlacing();
      }
    };

    const finish = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
      window.removeEventListener('keydown', onKey);
      stop.current = null;
      tree.mark(null);
      setLabel(null);
    };

    stop.current = () => {
      finish();
      if (held.dragging) {
        store().cancelPlacing();
      }
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    window.addEventListener('keydown', onKey);
  };

  return { active, label, press };
}
