// A focusable separator with a value is the ARIA window splitter pattern,
// which these two rules do not recognise as interactive.
/* eslint-disable jsx-a11y/no-noninteractive-element-interactions, jsx-a11y/no-noninteractive-tabindex */
import { useRef, type JSX, type KeyboardEvent, type PointerEvent } from 'react';

import { MAX_LIST_HEIGHT, MIN_LIST_HEIGHT } from '../../lib/prefs';
import { strings } from '../../lib/strings';
import { useUiStore } from '../../state/uiStore';
import { draggedHeight, keyedHeight } from './model';

import './ListResizer.scss';

interface ListResizerProps {
  list: 'toolpaths' | 'components';
  /** The id of the list section the divider sizes. */
  controls: string;
  /** The list's height on screen now, for a drag that starts from shared space. */
  measure: () => number;
}

/** The divider above a dock list: drag it (or use the arrow keys) to show more rows. */
export function ListResizer({ list, controls, measure }: ListResizerProps): JSX.Element {
  const height = useUiStore((state) =>
    list === 'toolpaths' ? state.toolpathListHeight : state.componentTreeHeight,
  );
  const setListHeight = useUiStore((state) => state.setListHeight);
  const drag = useRef<{ y: number; height: number } | null>(null);
  const label = list === 'toolpaths' ? strings.docks.resizeToolpathList : strings.docks.resizeTree;

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { y: event.clientY, height: height > 0 ? height : measure() };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (start !== null) {
      setListHeight(list, draggedHeight(start.height, event.clientY - start.y));
    }
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = keyedHeight(height > 0 ? height : measure(), event.key, event.shiftKey);
    if (next !== null) {
      event.preventDefault();
      setListHeight(list, next);
    }
  };

  return (
    <div
      aria-controls={controls}
      aria-label={label}
      aria-orientation="horizontal"
      aria-valuemax={MAX_LIST_HEIGHT}
      aria-valuemin={MIN_LIST_HEIGHT}
      aria-valuenow={height > 0 ? height : undefined}
      className="list-resizer"
      onDoubleClick={() => setListHeight(list, 0)}
      onKeyDown={onKeyDown}
      onLostPointerCapture={() => (drag.current = null)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      role="separator"
      tabIndex={0}
      data-tip={label}
    />
  );
}
