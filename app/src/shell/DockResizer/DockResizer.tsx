// A focusable separator with a value is the ARIA window splitter pattern,
// which these two rules do not recognise as interactive.
/* eslint-disable jsx-a11y/no-noninteractive-element-interactions, jsx-a11y/no-noninteractive-tabindex */
import { useRef, type JSX, type KeyboardEvent, type PointerEvent } from 'react';

import { strings } from '../../lib/strings';
import { MAX_DOCK_WIDTH, MIN_DOCK_WIDTH } from '../../lib/prefs';
import { useUiStore } from '../../state/uiStore';
import { draggedWidth, keyedWidth, type DockSide } from './model';

import './DockResizer.scss';

interface DockResizerProps {
  side: DockSide;
  /** The id of the panel this edge sizes. */
  controls: string;
}

/** The draggable inner edge of a dock panel; arrow keys move it too. */
export function DockResizer({ side, controls }: DockResizerProps): JSX.Element {
  const width = useUiStore((state) => (side === 'left' ? state.leftWidth : state.rightWidth));
  const setDockWidth = useUiStore((state) => state.setDockWidth);
  const drag = useRef<{ x: number; width: number } | null>(null);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { x: event.clientX, width };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (start !== null) {
      setDockWidth(side, draggedWidth(side, start.width, event.clientX - start.x));
    }
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = keyedWidth(side, width, event.key, event.shiftKey);
    if (next !== null) {
      event.preventDefault();
      setDockWidth(side, next);
    }
  };

  return (
    <div
      aria-controls={controls}
      aria-label={side === 'left' ? strings.docks.resizeLeft : strings.docks.resizeRight}
      aria-orientation="vertical"
      aria-valuemax={MAX_DOCK_WIDTH}
      aria-valuemin={MIN_DOCK_WIDTH}
      aria-valuenow={width}
      className={`dock-resizer dock-resizer--${side}`}
      onKeyDown={onKeyDown}
      onLostPointerCapture={() => (drag.current = null)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      role="separator"
      tabIndex={0}
      data-tip={side === 'left' ? strings.docks.resizeLeft : strings.docks.resizeRight}
    />
  );
}
