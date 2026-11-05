import { useEffect, useRef, useState, type JSX, type PointerEvent } from 'react';

import { jointText } from '../../forms/FurnitureDesigner/jointItems';
import { furnitureStrings } from '../../lib/strings/furniture';
import { useFurnitureStore } from '../../state/furnitureStore';
import { JointPopover } from '../JointPopover/JointPopover';
import { useDocumentStore } from '../../state/documentStore';
import { MM_PER_INCH } from '../../lib/units';
import { draggedSize } from '../View3D/furnitureSeams';
import type { DimensionLabel, SeamLabel, ShownArrow } from '../View3D/useFurnitureView';

import './FurnitureOverlay.scss';

const text = furnitureStrings.furniture;

interface FurnitureOverlayProps {
  labels: readonly DimensionLabel[];
  seamLabel: SeamLabel | null;
  arrows: readonly ShownArrow[];
}

/** A size arrow's drag: where it started and how screen pixels map to the size. */
interface ArrowDrag {
  key: string;
  pointerId: number;
  x: number;
  y: number;
  /** The arrow's unit direction on screen. */
  dx: number;
  dy: number;
  mmPerPx: number;
  start: number;
}

/** Arrow heads: a short V at the tip, pixels. */
function headPath(a: ShownArrow): string {
  const len = Math.hypot(a.x2 - a.x1, a.y2 - a.y1) || 1;
  const ux = (a.x2 - a.x1) / len;
  const uy = (a.y2 - a.y1) / len;
  const size = 9;
  const bx = a.x2 - ux * size;
  const by = a.y2 - uy * size;
  return `M${bx - uy * size * 0.6} ${by + ux * size * 0.6}L${a.x2} ${a.y2}L${bx + uy * size * 0.6} ${by - ux * size * 0.6}`;
}

/**
 * Over the 3D view while a design is open: the overall dimension lengths,
 * the hovered seam's joint name, and the seam popover after a click.
 */
export function FurnitureOverlay({
  labels,
  seamLabel,
  arrows,
}: FurnitureOverlayProps): JSX.Element {
  const ref = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ width: 800, height: 600 });
  const connections = useFurnitureStore((s) => s.connections);
  const popover = useFurnitureStore((s) => s.popover);
  const preview = useFurnitureStore((s) => s.preview);
  const session = useFurnitureStore((s) => s.session);
  const units = useDocumentStore((s) => s.job.units);
  const dragRef = useRef<ArrowDrag | null>(null);

  const onDown = (arrow: ShownArrow, event: PointerEvent<SVGElement>) => {
    const current = useFurnitureStore.getState().session?.values[arrow.key];
    const len = Math.hypot(arrow.x2 - arrow.x1, arrow.y2 - arrow.y1);
    if (typeof current !== 'number' || len < 1) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      key: arrow.key,
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      dx: (arrow.x2 - arrow.x1) / len,
      dy: (arrow.y2 - arrow.y1) / len,
      mmPerPx: arrow.length / len,
      start: current,
    };
  };
  const onMove = (event: PointerEvent<SVGElement>) => {
    const drag = dragRef.current;
    const state = useFurnitureStore.getState();
    if (drag === null || drag.pointerId !== event.pointerId || state.session === null) {
      return;
    }
    const param = state.session.template.params.find((p) => p.key === drag.key);
    const along = (event.clientX - drag.x) * drag.dx + (event.clientY - drag.y) * drag.dy;
    const step = units === 'inch' ? MM_PER_INCH / 16 : 1;
    const value = draggedSize(drag.start, along, drag.mmPerPx, step, param?.min, param?.max);
    if (value !== state.session.values[drag.key]) {
      state.setValues({ ...state.session.values, [drag.key]: value });
    }
  };
  const onUp = (event: PointerEvent<SVGElement>) => {
    if (dragRef.current?.pointerId === event.pointerId) {
      dragRef.current = null;
      useFurnitureStore.getState().release();
    }
  };

  useEffect(() => {
    const element = ref.current;
    if (element === null) {
      return undefined;
    }
    const observer = new ResizeObserver(() => {
      setSize({ width: element.clientWidth, height: element.clientHeight });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const hovered = seamLabel === null ? undefined : connections.find((c) => c.key === seamLabel.key);
  const open = popover === null ? undefined : connections.find((c) => c.key === popover.key);
  const classLabel =
    open === undefined
      ? ''
      : (session?.template.jointClasses?.[open.class]?.label ?? text.designer.otherClass);

  return (
    <div className="furniture-overlay" ref={ref}>
      <svg className="furniture-overlay__arrows">
        {arrows.map((arrow) => (
          <g
            className="furniture-overlay__arrow"
            key={arrow.axis}
            onPointerCancel={onUp}
            onPointerDown={(event) => onDown(arrow, event)}
            onPointerMove={onMove}
            onPointerUp={onUp}
          >
            <title>{text.drag[arrow.axis]}</title>
            <line
              className="furniture-overlay__arrow-hit"
              x1={arrow.x1}
              x2={arrow.x2}
              y1={arrow.y1}
              y2={arrow.y2}
            />
            <line
              className="furniture-overlay__arrow-line"
              x1={arrow.x1}
              x2={arrow.x2}
              y1={arrow.y1}
              y2={arrow.y2}
            />
            <path className="furniture-overlay__arrow-line" d={headPath(arrow)} />
          </g>
        ))}
      </svg>
      {labels.map((label) => (
        <span
          className="furniture-overlay__dimension"
          key={label.axis}
          style={{ transform: `translate(${label.x}px, ${label.y}px)` }}
        >
          {`${text.designer.dimensions[label.axis]} ${label.text}`}
        </span>
      ))}
      {hovered === undefined || seamLabel === null ? null : (
        <span
          className="furniture-overlay__seam"
          role="status"
          style={{ transform: `translate(${seamLabel.x}px, ${seamLabel.y}px)` }}
        >
          {text.seam.hover(jointText(hovered.joint).name)}
        </span>
      )}
      {open === undefined || popover === null ? null : (
        <JointPopover
          classLabel={classLabel}
          key={open.key}
          panels={preview?.panels ?? []}
          seam={open}
          viewHeight={size.height}
          viewWidth={size.width}
          x={popover.x}
          y={popover.y}
        />
      )}
    </div>
  );
}
