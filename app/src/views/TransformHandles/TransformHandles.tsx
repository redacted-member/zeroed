import type { JSX, PointerEvent as ReactPointerEvent } from 'react';

import { strings } from '../../lib/formStrings';
import type { AnyHandle } from '../View3D/handleValues';
import { handleKey, type XY } from '../View3D/transformHandles';

import './TransformHandles.scss';

const text = strings.model.transform3d;
/** Half the side of a square handle, CSS pixels. */
const HALF = 5;
const RADIUS = 6;

/** A handle where it shows on the canvas. */
export interface ShownHandle {
  handle: AnyHandle;
  at: XY;
}

interface TransformHandlesProps {
  /** The box corners on the canvas, in order. */
  outline: readonly XY[];
  handles: readonly ShownHandle[];
  /** Where the rotation arrow's stalk meets the top edge. */
  stalk: XY | null;
  /** The height post, from the zero plane to the Total Height handle. */
  post: { from: XY; to: XY } | null;
  /** The middle handle marks the rotation centre. */
  centreMode: boolean;
  /** The handle whose values the value box shows. */
  active: AnyHandle | null;
  onHandleDown: (handle: AnyHandle, event: ReactPointerEvent<SVGElement>) => void;
  onHandleMove: (event: ReactPointerEvent<SVGElement>) => void;
  onHandleUp: (event: ReactPointerEvent<SVGElement>) => void;
  onHandleDoubleClick: (handle: AnyHandle) => void;
}

function label(handle: AnyHandle, centreMode: boolean): string {
  switch (handle.kind) {
    case 'move':
      return centreMode ? text.centre : text.move;
    case 'corner':
      return text.scale;
    case 'edge':
      return text.stretch;
    case 'rotate':
      return text.rotate;
    case 'base':
      return text.base;
    case 'shape':
      return text.shape;
    case 'total':
      return text.total;
    case 'profile':
      return text.profile;
  }
}

function sameHandle(a: AnyHandle | null, b: AnyHandle): boolean {
  return a !== null && handleKey(a) === handleKey(b);
}

/** The move, scale, stretch, rotate and height handles of a component
 * drawn over the 3D view. The view does the dragging. */
export function TransformHandles({
  outline,
  handles,
  stalk,
  post,
  centreMode,
  active,
  onHandleDown,
  onHandleMove,
  onHandleUp,
  onHandleDoubleClick,
}: TransformHandlesProps): JSX.Element {
  const rotate = handles.find((h) => h.handle.kind === 'rotate');
  return (
    <svg aria-label={text.label} className="transform-handles" role="group">
      <polygon
        className="transform-handles__outline"
        points={outline.map((p) => `${p.x},${p.y}`).join(' ')}
      />
      {rotate !== undefined && stalk !== null ? (
        <line
          className="transform-handles__stalk"
          x1={stalk.x}
          x2={rotate.at.x}
          y1={stalk.y}
          y2={rotate.at.y}
        />
      ) : null}
      {post === null ? null : (
        <line
          aria-label={text.post}
          className="transform-handles__post"
          x1={post.from.x}
          x2={post.to.x}
          y1={post.from.y}
          y2={post.to.y}
        />
      )}
      {handles.map(({ handle, at }) => {
        const kind = handle.kind === 'move' && centreMode ? 'centre' : handle.kind;
        const activeClass = sameHandle(active, handle) ? ' transform-handles__handle--active' : '';
        const common = {
          className: `transform-handles__handle transform-handles__handle--${kind}${activeClass}`,
          onDoubleClick: () => onHandleDoubleClick(handle),
          onPointerCancel: onHandleUp,
          onPointerDown: (event: ReactPointerEvent<SVGElement>) => onHandleDown(handle, event),
          onPointerMove: onHandleMove,
          onPointerUp: onHandleUp,
        };
        const title = <title>{label(handle, centreMode)}</title>;
        switch (handle.kind) {
          case 'move':
          case 'rotate':
            return (
              <circle {...common} cx={at.x} cy={at.y} key={handleKey(handle)} r={RADIUS}>
                {title}
              </circle>
            );
          case 'profile':
            return (
              <polygon
                {...common}
                key={handleKey(handle)}
                points={`${at.x},${at.y - RADIUS} ${at.x + RADIUS},${at.y} ${at.x},${at.y + RADIUS} ${at.x - RADIUS},${at.y}`}
              >
                {title}
              </polygon>
            );
          case 'total':
            return (
              <polygon
                {...common}
                key={handleKey(handle)}
                points={`${at.x},${at.y - RADIUS} ${at.x + RADIUS},${at.y + HALF} ${at.x - RADIUS},${at.y + HALF}`}
              >
                {title}
              </polygon>
            );
          default:
            return (
              <rect
                {...common}
                height={HALF * 2}
                key={handleKey(handle)}
                width={HALF * 2}
                x={at.x - HALF}
                y={at.y - HALF}
              >
                {title}
              </rect>
            );
        }
      })}
    </svg>
  );
}
