import type { JSX, PointerEvent as ReactPointerEvent } from 'react';

import { strings } from '../../lib/formStrings';
import type { PlaneGrip } from '../View3D/planeHandles';
import type { ShownPlaneHandles } from '../View3D/usePlaneHandles';

import './PlaneHandles.scss';

const text = strings.model.importModel.planeHandles;

interface PlaneHandlesProps {
  shown: ShownPlaneHandles;
  active: PlaneGrip | null;
  onDown: (grip: PlaneGrip, event: ReactPointerEvent<SVGElement>) => void;
  onMove: (event: ReactPointerEvent<SVGElement>) => void;
  onUp: (event: ReactPointerEvent<SVGElement>) => void;
}

/** The segmenting plane's handles over the 3D view: an arrow along its
 * normal to move it and two knobs on it to tilt it. The view drags them. */
export function PlaneHandles({
  shown,
  active,
  onDown,
  onMove,
  onUp,
}: PlaneHandlesProps): JSX.Element {
  const { centre, move, tiltU, tiltV } = shown;
  const common = { onPointerCancel: onUp, onPointerMove: onMove, onPointerUp: onUp };
  const cls = (grip: PlaneGrip, base: string) =>
    active === grip ? `${base} plane-handles__grip--active` : base;
  return (
    <svg aria-label={text.label} className="plane-handles" role="group">
      <line className="plane-handles__stem" x1={centre.x} x2={move.x} y1={centre.y} y2={move.y} />
      {(['tiltU', 'tiltV'] as const).map((grip) => {
        const at = grip === 'tiltU' ? tiltU : tiltV;
        return (
          <g key={grip}>
            <line className="plane-handles__arm" x1={centre.x} x2={at.x} y1={centre.y} y2={at.y} />
            <circle
              {...common}
              aria-label={text.tilt}
              className={cls(grip, 'plane-handles__grip plane-handles__grip--tilt')}
              cx={at.x}
              cy={at.y}
              onPointerDown={(event) => onDown(grip, event)}
              r={7}
            >
              <title>{text.tilt}</title>
            </circle>
          </g>
        );
      })}
      <circle
        {...common}
        aria-label={text.move}
        className={cls('move', 'plane-handles__grip plane-handles__grip--move')}
        cx={move.x}
        cy={move.y}
        onPointerDown={(event) => onDown('move', event)}
        r={8}
      >
        <title>{text.move}</title>
      </circle>
    </svg>
  );
}
