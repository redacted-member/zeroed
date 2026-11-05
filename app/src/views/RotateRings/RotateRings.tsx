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

import './RotateRings.scss';

const text = strings.model.importModel;

/** A ring where it shows on the canvas: an SVG path in CSS pixels. */
export interface ShownRing {
  axis: RingAxis;
  path: string;
}

/** What a pointer grabbed: a ring, the ball in the middle, or the cube in
 * the ball that moves the rotation axis. */
export type RingTarget = RingAxis | 'ball' | 'axis';

interface RotateRingsProps {
  rings: readonly ShownRing[];
  ball: { x: number; y: number; r: number } | null;
  /** Rotation Axis Movement on: the cube, its middle and half side. */
  cube: { x: number; y: number; r: number } | null;
  /** The ring or ball being dragged. */
  active: RingTarget | null;
  /** The angle box: which axis, its angle now and where it shows. */
  field: { axis: RingAxis; degrees: number; x: number; y: number } | null;
  onDown: (target: RingTarget, event: ReactPointerEvent<SVGElement>) => void;
  onMove: (event: ReactPointerEvent<SVGElement>) => void;
  onUp: (event: ReactPointerEvent<SVGElement>) => void;
  onAngle: (axis: RingAxis, degrees: number) => void;
  onCloseField: () => void;
}

function axisName(axis: RingAxis): string {
  return axis.toUpperCase();
}

/** The red, green and blue rotation rings and the grey ball drawn over the
 * 3D view while a model is orientated. The view does the dragging. */
export function RotateRings({
  rings,
  ball,
  cube,
  active,
  field,
  onDown,
  onMove,
  onUp,
  onAngle,
  onCloseField,
}: RotateRingsProps): JSX.Element {
  const common = { onPointerCancel: onUp, onPointerMove: onMove, onPointerUp: onUp };
  return (
    <>
      <svg aria-label={text.rings} className="rotate-rings" role="group">
        {rings.map(({ axis, path }) => (
          <path
            {...common}
            className={`rotate-rings__ring rotate-rings__ring--${axis}${
              active === axis ? ' rotate-rings__ring--active' : ''
            }`}
            d={path}
            key={axis}
            onPointerDown={(event) => onDown(axis, event)}
          >
            <title>{text.ring(axisName(axis))}</title>
          </path>
        ))}
        {ball === null ? null : (
          <circle
            {...common}
            className={`rotate-rings__ball${active === 'ball' ? ' rotate-rings__ball--active' : ''}`}
            cx={ball.x}
            cy={ball.y}
            onPointerDown={(event) => onDown('ball', event)}
            r={ball.r}
          >
            <title>{text.ball}</title>
          </circle>
        )}
        {cube === null ? null : (
          <rect
            {...common}
            className={`rotate-rings__cube${active === 'axis' ? ' rotate-rings__cube--active' : ''}`}
            height={cube.r * 2}
            onPointerDown={(event) => onDown('axis', event)}
            width={cube.r * 2}
            x={cube.x - cube.r}
            y={cube.y - cube.r}
          >
            <title>{text.axisCube}</title>
          </rect>
        )}
      </svg>
      {field === null ? null : (
        <AngleField
          axis={field.axis}
          degrees={field.degrees}
          key={`${field.axis}:${field.degrees}`}
          onApply={onAngle}
          onClose={onCloseField}
          x={field.x}
          y={field.y}
        />
      )}
    </>
  );
}

interface AngleFieldProps {
  axis: RingAxis;
  degrees: number;
  x: number;
  y: number;
  onApply: (axis: RingAxis, degrees: number) => void;
  onClose: () => void;
}

/** The floating box an angle is typed into, under the pointer. */
function AngleField({ axis, degrees, x, y, onApply, onClose }: AngleFieldProps): JSX.Element {
  const [value, setValue] = useState(String(degrees));
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  // The box opens on a click on its ring, ready for the angle.
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  const typed = Number(value.trim());
  const valid = value.trim() !== '' && Number.isFinite(typed);
  return (
    <form
      className="rotate-rings__field"
      // The box follows the pointer: a computed position.
      style={{ left: `${x}px`, top: `${y}px` }}
      onSubmit={(event) => {
        event.preventDefault();
        if (valid) {
          onApply(axis, typed);
        }
      }}
    >
      <label className="rotate-rings__label" htmlFor={id}>
        {text.ringAngle(axisName(axis))}
      </label>
      <input
        aria-invalid={!valid}
        className="rotate-rings__input"
        id={id}
        inputMode="decimal"
        onChange={(event) => setValue(event.target.value)}
        ref={input}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === 'Escape') {
            onClose();
          }
        }}
        value={value}
      />
      <span className="rotate-rings__unit">°</span>
    </form>
  );
}
