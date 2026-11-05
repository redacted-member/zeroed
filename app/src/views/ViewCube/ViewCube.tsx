import type { JSX, KeyboardEvent } from 'react';

import { strings } from '../../lib/formStrings';
import { projectCube, type CubeFace } from './model';

import './ViewCube.scss';

const SIZE = 76;
const HALF_EDGE = 21;
/** Labels only show on faces turned this far towards the camera. */
const LABEL_FACING = 0.4;

const text = strings.view3dBar;

const NAMES: Readonly<Record<CubeFace, string>> = {
  top: text.top,
  bottom: text.bottom,
  front: text.front,
  back: text.back,
  left: text.left,
  right: text.right,
};

function tone(facing: number): string {
  if (facing > 0.8) {
    return 'view-cube__face view-cube__face--lit';
  }
  return facing > 0.45 ? 'view-cube__face view-cube__face--mid' : 'view-cube__face';
}

export interface ViewCubeProps {
  theta: number;
  phi: number;
  onPick: (face: CubeFace) => void;
}

/** A small cube turned like the 3D camera; picking a face looks at it. */
export function ViewCube({ theta, phi, onPick }: ViewCubeProps): JSX.Element {
  const faces = projectCube(theta, phi, HALF_EDGE);
  const onKey = (event: KeyboardEvent<SVGGElement>, face: CubeFace) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onPick(face);
    }
  };
  return (
    <svg
      aria-label={text.cube}
      className="view-cube"
      height={SIZE}
      role="group"
      viewBox={`${-SIZE / 2} ${-SIZE / 2} ${SIZE} ${SIZE}`}
      width={SIZE}
    >
      {faces.map(({ face, points, centre, facing }) => (
        <g
          aria-label={text.viewHint(NAMES[face])}
          className="view-cube__button"
          data-tip={text.viewHint(NAMES[face])}
          key={face}
          onClick={() => onPick(face)}
          onKeyDown={(event) => onKey(event, face)}
          role="button"
          tabIndex={0}
        >
          <polygon className={tone(facing)} points={points} />
          {facing >= LABEL_FACING ? (
            <text className="view-cube__label" x={centre.x} y={centre.y}>
              {NAMES[face]}
            </text>
          ) : null}
        </g>
      ))}
    </svg>
  );
}
