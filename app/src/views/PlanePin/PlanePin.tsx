import type { JSX, PointerEvent as ReactPointerEvent } from 'react';

import { strings } from '../../lib/formStrings';

import './PlanePin.scss';

/** Length of the pin's stalk above the plane, CSS pixels. */
const STALK = 28;
const HEAD = 7;

interface PlanePinProps {
  /** Where the middle of the plane shows on the canvas. */
  x: number;
  y: number;
  onPointerDown: (event: ReactPointerEvent<SVGElement>) => void;
  onPointerMove: (event: ReactPointerEvent<SVGElement>) => void;
  onPointerUp: (event: ReactPointerEvent<SVGElement>) => void;
}

/** The blue pin standing on the Replace Below plane in the 3D view: drag it
 * up or down to move the plane. */
export function PlanePin({
  x,
  y,
  onPointerDown,
  onPointerMove,
  onPointerUp,
}: PlanePinProps): JSX.Element {
  return (
    <svg aria-label={strings.model.replaceBelow.pin} className="plane-pin" role="group">
      <line className="plane-pin__stalk" x1={x} x2={x} y1={y} y2={y - STALK} />
      <circle
        className="plane-pin__head"
        cx={x}
        cy={y - STALK}
        onPointerCancel={onPointerUp}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        r={HEAD}
      >
        <title>{strings.model.replaceBelow.pin}</title>
      </circle>
    </svg>
  );
}
