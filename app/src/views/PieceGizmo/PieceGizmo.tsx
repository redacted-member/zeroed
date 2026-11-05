import {
  useEffect,
  useId,
  useRef,
  useState,
  type JSX,
  type PointerEvent as ReactPointerEvent,
} from 'react';

import { strings } from '../../lib/formStrings';
import type { RingAxis } from '../View3D/rotateRings';

import './PieceGizmo.scss';

const text = strings.assembly;

/** What a pointer grabbed: a move arrow, a rotation ring or the ball. */
export type GizmoTarget =
  { kind: 'move'; axis: RingAxis } | { kind: 'turn'; axis: RingAxis } | { kind: 'ball' };

/** An arrow where it shows: from the middle to its tip, CSS pixels. */
export interface ShownArrow {
  axis: RingAxis;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** A ring where it shows: an SVG path in CSS pixels. */
export interface ShownGizmoRing {
  axis: RingAxis;
  path: string;
}

export interface ShownGizmo {
  arrows: ShownArrow[];
  rings: ShownGizmoRing[];
  ball: { x: number; y: number; r: number };
}

/** The number box: what it edits, the value it shows and where. */
export interface GizmoField {
  kind: 'move' | 'turn';
  axis: RingAxis;
  value: number;
  x: number;
  y: number;
  /** A drag shows its distance or angle here; a click lets one be typed. */
  editable: boolean;
}

interface PieceGizmoProps {
  shown: ShownGizmo;
  active: GizmoTarget | null;
  field: GizmoField | null;
  /** The job unit's name, for distances. */
  unit: string;
  onDown: (target: GizmoTarget, event: ReactPointerEvent<SVGElement>) => void;
  onMove: (event: ReactPointerEvent<SVGElement>) => void;
  onUp: (event: ReactPointerEvent<SVGElement>) => void;
  onValue: (kind: 'move' | 'turn', axis: RingAxis, value: number) => void;
  onCloseField: () => void;
}

function axisName(axis: RingAxis): string {
  return axis.toUpperCase();
}

function same(a: GizmoTarget | null, b: GizmoTarget): boolean {
  return (
    a !== null && a.kind === b.kind && (a.kind === 'ball' || b.kind === 'ball' || a.axis === b.axis)
  );
}

/** Arrow head: a small triangle at the tip pointing away from the middle. */
function head(arrow: ShownArrow): string {
  const dx = arrow.x2 - arrow.x1;
  const dy = arrow.y2 - arrow.y1;
  const length = Math.hypot(dx, dy) || 1;
  const ux = dx / length;
  const uy = dy / length;
  const size = 10;
  const baseX = arrow.x2 - ux * size;
  const baseY = arrow.y2 - uy * size;
  const px = -uy * size * 0.5;
  const py = ux * size * 0.5;
  return `M${arrow.x2} ${arrow.y2}L${baseX + px} ${baseY + py}L${baseX - px} ${baseY - py}Z`;
}

/**
 * The move arrows, rotation rings and middle ball drawn over the picked
 * piece in the 3D view, and the number box that shows or takes a distance
 * or angle. The view does the dragging.
 */
export function PieceGizmo({
  shown,
  active,
  field,
  unit,
  onDown,
  onMove,
  onUp,
  onValue,
  onCloseField,
}: PieceGizmoProps): JSX.Element {
  const common = { onPointerCancel: onUp, onPointerMove: onMove, onPointerUp: onUp };
  return (
    <>
      <svg aria-label={text.handles} className="piece-gizmo" role="group">
        {shown.rings.map(({ axis, path }) => {
          const target: GizmoTarget = { kind: 'turn', axis };
          return (
            <path
              {...common}
              className={`piece-gizmo__ring piece-gizmo__ring--${axis}${
                same(active, target) ? ' piece-gizmo__ring--active' : ''
              }`}
              d={path}
              key={`ring-${axis}`}
              onPointerDown={(event) => onDown(target, event)}
            >
              <title>{text.ring(axisName(axis))}</title>
            </path>
          );
        })}
        <circle
          {...common}
          className={`piece-gizmo__ball${
            same(active, { kind: 'ball' }) ? ' piece-gizmo__ball--active' : ''
          }`}
          cx={shown.ball.x}
          cy={shown.ball.y}
          onPointerDown={(event) => onDown({ kind: 'ball' }, event)}
          r={shown.ball.r}
        >
          <title>{text.ball}</title>
        </circle>
        {shown.arrows.map((arrow) => {
          const target: GizmoTarget = { kind: 'move', axis: arrow.axis };
          return (
            <g
              {...common}
              className={`piece-gizmo__arrow piece-gizmo__arrow--${arrow.axis}${
                same(active, target) ? ' piece-gizmo__arrow--active' : ''
              }`}
              key={`arrow-${arrow.axis}`}
              onPointerDown={(event) => onDown(target, event)}
            >
              <title>{text.arrow(axisName(arrow.axis))}</title>
              <line
                className="piece-gizmo__shaft"
                x1={arrow.x1}
                x2={arrow.x2}
                y1={arrow.y1}
                y2={arrow.y2}
              />
              <path className="piece-gizmo__head" d={head(arrow)} />
            </g>
          );
        })}
      </svg>
      {field === null ? null : (
        <ValueField
          field={field}
          key={`${field.kind}:${field.axis}:${field.editable ? 'edit' : field.value}`}
          onApply={onValue}
          onClose={onCloseField}
          unit={field.kind === 'turn' ? '°' : unit}
        />
      )}
    </>
  );
}

interface ValueFieldProps {
  field: GizmoField;
  unit: string;
  onApply: (kind: 'move' | 'turn', axis: RingAxis, value: number) => void;
  onClose: () => void;
}

/** The floating box by the pointer: live while dragging, typed after a click. */
function ValueField({ field, unit, onApply, onClose }: ValueFieldProps): JSX.Element {
  const [value, setValue] = useState(String(field.value));
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (field.editable) {
      input.current?.focus();
      input.current?.select();
    }
  }, [field.editable]);
  const typed = Number(value.trim());
  const valid = value.trim() !== '' && Number.isFinite(typed);
  const label =
    field.kind === 'move' ? text.moveBy(axisName(field.axis)) : text.turnBy(axisName(field.axis));
  return (
    <form
      className="piece-gizmo__field"
      // The box follows the pointer: a computed position.
      style={{ left: `${field.x}px`, top: `${field.y}px` }}
      onSubmit={(event) => {
        event.preventDefault();
        if (valid && field.editable) {
          onApply(field.kind, field.axis, typed);
        }
      }}
    >
      <label className="piece-gizmo__label" htmlFor={id}>
        {label}
      </label>
      <input
        aria-invalid={!valid}
        className="piece-gizmo__input"
        id={id}
        inputMode="decimal"
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === 'Escape') {
            onClose();
          }
        }}
        readOnly={!field.editable}
        ref={input}
        value={value}
      />
      <span className="piece-gizmo__unit">{unit}</span>
    </form>
  );
}
