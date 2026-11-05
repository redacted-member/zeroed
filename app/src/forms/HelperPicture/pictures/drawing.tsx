import type { JSX } from 'react';

import { strings } from '../../../lib/formStrings';
import { Dim, K, PathHead, type Picture } from '../kit';

// Drawing and editing form pictures: vectors in blue, the result of the
// tool drawn over the original (dashed). viewBox 64 x 48, snap icons 24 x 24.

const t = strings.helpers.drawing;

export type SnapKind =
  'nodes' | 'midpoints' | 'centres' | 'intersections' | 'jobPoints' | 'guides' | 'grid' | 'smart';

const SNAP_DRAWINGS: Record<SnapKind, () => JSX.Element> = {
  nodes: () => (
    <>
      <path className={K.vector} d="M3 20 L12 6 L21 14" />
      <rect className={K.dot} height={5} width={5} x={9.5} y={3.5} />
    </>
  ),
  midpoints: () => (
    <>
      <line className={K.vector} x1={3} x2={21} y1={16} y2={16} />
      <polygon className={K.dot} points="12,11 15,16 9,16" />
    </>
  ),
  centres: () => (
    <>
      <circle className={K.vector} cx={12} cy={12} r={8} />
      <circle className={K.dot} cx={12} cy={12} r={2.2} />
    </>
  ),
  intersections: () => (
    <>
      <line className={K.vector} x1={3} x2={21} y1={5} y2={19} />
      <line className={K.vector} x1={3} x2={21} y1={19} y2={5} />
      <circle className={K.dot} cx={12} cy={12} r={2.4} />
    </>
  ),
  jobPoints: () => (
    <>
      <rect className={K.line} height={14} width={18} x={3} y={5} />
      <circle className={K.dot} cx={3} cy={19} r={1.8} />
      <circle className={K.dot} cx={21} cy={5} r={1.8} />
      <circle className={K.dot} cx={12} cy={12} r={1.8} />
    </>
  ),
  guides: () => (
    <>
      <line className={K.faint} x1={2} x2={22} y1={9} y2={9} />
      <line className={K.faint} x1={15} x2={15} y1={2} y2={22} />
      <circle className={K.dot} cx={15} cy={9} r={2} />
    </>
  ),
  grid: () => (
    <>
      {[5, 12, 19].map((y) =>
        [5, 12, 19].map((x) => (
          <circle className={K.mark} cx={x} cy={y} key={`${x}-${y}`} r={1.3} />
        )),
      )}
      <circle className={K.dot} cx={12} cy={12} r={2.2} />
    </>
  ),
  smart: () => (
    <>
      <rect className={K.mark} height={4} width={4} x={2} y={4} />
      <line className={K.faint} x1={6} x2={20} y1={6} y2={6} />
      <line className={K.faint} x1={18} x2={18} y1={6} y2={20} />
      <circle className={K.dot} cx={18} cy={6} r={2} />
    </>
  ),
};

/** Snap settings: a small icon for each kind of point the cursor snaps to. */
export function snapPicture(kind: SnapKind): Picture {
  return { id: `snap-${kind}`, alt: t.snap[kind], viewBox: '0 0 24 24', draw: SNAP_DRAWINGS[kind] };
}

export type CornerKind = 'none' | 'external' | 'internal';

const CORNER_PATHS: Record<CornerKind, string> = {
  none: 'M10 8 H54 V40 H10 Z',
  external:
    'M18 8 H46 A8 8 0 0 1 54 16 V32 A8 8 0 0 1 46 40 H18 A8 8 0 0 1 10 32 V16 A8 8 0 0 1 18 8 Z',
  internal:
    'M18 8 H46 A8 8 0 0 0 54 16 V32 A8 8 0 0 0 46 40 H18 A8 8 0 0 0 10 32 V16 A8 8 0 0 0 18 8 Z',
};

/** Draw Rectangle: square, rounded or inward cut corners. */
export function cornerPicture(kind: CornerKind): Picture {
  return {
    id: `corner-${kind}`,
    alt: t.corner[kind],
    draw: () => <path className={K.vector} d={CORNER_PATHS[kind]} />,
  };
}

export type ArcMode = 'threePoints' | 'centreStartEnd';

/** Draw Arc: through three points, or round a center from start to end. */
export function arcModePicture(mode: ArcMode): Picture {
  return {
    id: `arc-${mode}`,
    alt: t.arcMode[mode],
    draw: () => (
      <>
        <path className={K.vector} d="M10 38 A24 24 0 0 1 54 38" />
        <circle className={K.dot} cx={10} cy={38} r={2.2} />
        <circle className={K.dot} cx={54} cy={38} r={2.2} />
        {mode === 'threePoints' ? (
          <circle className={K.dot} cx={32} cy={21} r={2.2} />
        ) : (
          <>
            <path className={K.faint} d="M10 38 L32 44 L54 38" />
            <circle className={K.mark} cx={32} cy={44} r={2.2} />
          </>
        )}
      </>
    ),
  };
}

const HEXAGON = '50,24 41,39.6 23,39.6 14,24 23,8.4 41,8.4';

export type PolygonSize = 'radius' | 'diameter';

/** Draw Polygon: the size given as the radius or the diameter to the corners. */
export function polygonSizePicture(sizeBy: PolygonSize): Picture {
  return {
    id: `polygon-${sizeBy}`,
    alt: t.polygonSize[sizeBy],
    draw: () => (
      <>
        <polygon className={K.vector} points={HEXAGON} />
        {sizeBy === 'radius' ? (
          <Dim x1={32} x2={50} y1={24} y2={24} />
        ) : (
          <Dim x1={14} x2={50} y1={24} y2={24} />
        )}
        <circle className={K.mark} cx={32} cy={24} r={1.6} />
      </>
    ),
  };
}

/** Draw Star: the outer radius to the points and the inner radius between. */
export const star: Picture = {
  id: 'star-radii',
  alt: t.star,
  draw: () => (
    <>
      <polygon
        className={K.vector}
        points="32,4 36.7,17.5 51,17.8 39.6,26.5 43.8,40.2 32,32 20.2,40.2 24.4,26.5 13,17.8 27.3,17.5"
      />
      <Dim x1={32} x2={32} y1={24} y2={4} />
      <Dim x1={32} x2={39.6} y1={24} y2={26.5} />
    </>
  ),
};

export type OffsetSide = 'outward' | 'inward';
export type OffsetCorner = 'round' | 'sharp';

/** Offset: the copy grown outward or shrunk inward, its corners round or sharp. */
export function offsetPicture(side: OffsetSide, corner: OffsetCorner): Picture {
  const key = side === 'inward' ? 'inward' : corner === 'sharp' ? 'outwardSharp' : 'outwardRound';
  return {
    id: `offset-${side}-${side === 'inward' ? 'any' : corner}`,
    alt: t.offset[key],
    draw: () => (
      <>
        <rect className={K.faint} height={24} width={36} x={14} y={12} />
        {side === 'inward' ? (
          <rect className={K.vector} height={12} width={24} x={20} y={18} />
        ) : (
          <rect
            className={K.vector}
            height={36}
            rx={corner === 'sharp' ? 0 : 6}
            width={48}
            x={8}
            y={6}
          />
        )}
      </>
    ),
  };
}

export type FilletKind = 'normal' | 'dogbone' | 'tbone' | 'plasma';

const FILLET_PATHS: Record<FilletKind, string> = {
  normal: 'M60 12 H28 A12 12 0 0 0 16 24 V46',
  dogbone: 'M60 12 H22 A8.25 8.25 0 1 0 16 18 V46',
  tbone: 'M60 12 H26 A5 5 0 0 0 16 12 V46',
  plasma: 'M60 12 H16 L9 5 A3.5 3.5 0 1 0 5 9 L16 12 V46',
};

/** Fillet: a rounded corner, or a cut past the corner for square parts. */
export function filletPicture(kind: FilletKind): Picture {
  return {
    id: `fillet-${kind}`,
    alt: t.fillet[kind],
    draw: () => (
      <>
        <path className={K.faint} d="M60 12 H16 V46" />
        <path className={K.vector} d={FILLET_PATHS[kind]} />
      </>
    ),
  };
}

export type FitKind = 'arcs' | 'beziers' | 'lines';

const FIT_POINTS: ReadonlyArray<readonly [number, number]> = [
  [6, 36],
  [16, 22],
  [28, 16],
  [40, 22],
  [50, 34],
  [58, 30],
];

const FIT_PATHS: Record<FitKind, string> = {
  arcs: 'M6 36 A24 24 0 0 1 50 34 A6 6 0 0 1 58 30',
  beziers: 'M6 36 C12 22 22 14 30 16 S46 30 50 34 S56 32 58 30',
  lines: 'M6 36 L16 22 L28 16 L40 22 L50 34 L58 30',
};

/** Fit Curves: the points joined with arcs, smooth spans or straight lines. */
export function fitPicture(kind: FitKind): Picture {
  return {
    id: `fit-${kind}`,
    alt: t.fit[kind],
    draw: () => (
      <>
        <path className={K.vector} d={FIT_PATHS[kind]} />
        {FIT_POINTS.map(([x, y]) => (
          <circle className={K.mark} cx={x} cy={y} key={`${x}-${y}`} r={1.5} />
        ))}
      </>
    ),
  };
}

/** Rotate: the shape turning round its pivot. */
export const rotate: Picture = {
  id: 'rotate-pivot',
  alt: t.rotate,
  draw: () => (
    <>
      <rect className={K.faint} height={16} width={28} x={18} y={16} />
      <rect
        className={K.vector}
        height={16}
        transform="rotate(-30 32 24)"
        width={28}
        x={18}
        y={16}
      />
      <path className={K.path} d="M50 30 A20 20 0 0 0 50 14" />
      <PathHead x1={51} x2={50} y1={18} y2={14} />
      <circle className={K.dot} cx={32} cy={24} r={2.2} />
    </>
  ),
};

/** Circular copy: copies placed round a center. */
export const circularArray: Picture = {
  id: 'circular-array',
  alt: t.circularArray,
  draw: () => (
    <>
      <circle className={K.faint} cx={32} cy={24} r={16} />
      {[0, 60, 120, 180, 240, 300].map((angle) => (
        <rect
          className={K.vector}
          height={6}
          key={angle}
          transform={`rotate(${angle} 32 24)`}
          width={6}
          x={29}
          y={5}
        />
      ))}
      <circle className={K.dot} cx={32} cy={24} r={2} />
    </>
  ),
};

/** Copy along vectors: copies placed along a curve. */
export const copyAlong: Picture = {
  id: 'copy-along',
  alt: t.copyAlong,
  draw: () => (
    <>
      <path className={K.faint} d="M4 40 Q20 4 36 24 T62 12" />
      {[
        [8, 31],
        [18, 18],
        [29, 19],
        [40, 26],
        [52, 20],
      ].map(([x, y]) => (
        <circle className={K.vector} cx={x} cy={y} key={`${x}`} r={3} />
      ))}
    </>
  ),
};

export type NestCorner = 'bottomLeft' | 'bottomRight' | 'topLeft' | 'topRight';

const NEST_CORNER: Record<NestCorner, { x: number; y: number; sx: number; sy: number }> = {
  bottomLeft: { x: 8, y: 40, sx: 1, sy: -1 },
  bottomRight: { x: 56, y: 40, sx: -1, sy: -1 },
  topLeft: { x: 8, y: 8, sx: 1, sy: 1 },
  topRight: { x: 56, y: 8, sx: -1, sy: 1 },
};

/** Nest: the parts packed from the start corner of the sheet. */
export function nestCornerPicture(corner: NestCorner): Picture {
  const { x, y, sx, sy } = NEST_CORNER[corner];
  const part = (dx: number, dy: number, w: number, h: number) => {
    const left = sx > 0 ? x + dx : x - dx - w;
    const top = sy > 0 ? y + dy : y - dy - h;
    return <rect className={K.vector} height={h} key={`${dx}-${dy}`} width={w} x={left} y={top} />;
  };
  return {
    id: `nest-${corner}`,
    alt: t.nestCorner[corner],
    draw: () => (
      <>
        <rect className={K.material} height={32} width={48} x={8} y={8} />
        {part(2, 2, 14, 10)}
        {part(18, 2, 10, 10)}
        {part(2, 14, 10, 8)}
        <circle className={K.dot} cx={x} cy={y} r={2.4} />
      </>
    ),
  };
}

export type DistortMode = 'box' | 'curve' | 'twoCurves';

/** Distort: shapes bent by a box, along a curve or between two curves. */
export function distortPicture(mode: DistortMode): Picture {
  const guide: Record<DistortMode, JSX.Element> = {
    box: <path className={K.faint} d="M6 10 Q32 2 58 10 L54 40 Q32 46 10 40 Z" />,
    curve: <path className={K.faint} d="M4 38 Q32 6 60 38" />,
    twoCurves: (
      <>
        <path className={K.faint} d="M4 18 Q32 2 60 18" />
        <path className={K.faint} d="M4 42 Q32 30 60 42" />
      </>
    ),
  };
  const shape: Record<DistortMode, string> = {
    box: 'M14 14 Q32 8 50 14 L48 36 Q32 40 16 36 Z',
    curve: 'M12 30 Q32 6 52 30 L46 34 Q32 18 18 34 Z',
    twoCurves: 'M10 18 Q32 6 54 18 L54 38 Q32 30 10 38 Z',
  };
  return {
    id: `distort-${mode}`,
    alt: t.distort[mode],
    draw: () => (
      <>
        {guide[mode]}
        <path className={K.vector} d={shape[mode]} />
      </>
    ),
  };
}

/** Vector Texture: straight or wavy lines across the area. */
export function vectorTexturePicture(waves: boolean): Picture {
  const lines = [12, 20, 28, 36]
    .map((y) => (waves ? `M8 ${y} Q16 ${y - 5} 24 ${y} T40 ${y} T56 ${y}` : `M8 ${y} H56`))
    .join(' ');
  return {
    id: waves ? 'vector-texture-waves' : 'vector-texture-straight',
    alt: waves ? t.vectorTexture.waves : t.vectorTexture.straight,
    draw: () => (
      <>
        <rect className={K.faint} height={36} width={52} x={6} y={6} />
        <path className={K.vector} d={lines} />
      </>
    ),
  };
}

/** Vector Unwrapper: a curve drawn round a cylinder laid out flat. */
export const unwrap: Picture = {
  id: 'vector-unwrap',
  alt: t.unwrap,
  draw: () => (
    <>
      <path className={K.material} d="M6 8 H22 A3 10 0 0 1 22 28 H6 Z" />
      <ellipse className={K.side} cx={6} cy={18} rx={3} ry={10} />
      <path className={K.vector} d="M8 24 Q14 10 21 14" />
      <PathHead x1={28} x2={34} y1={30} y2={34} />
      <line className={K.path} x1={24} x2={34} y1={26} y2={34} />
      <rect className={K.material} height={14} width={24} x={36} y={30} />
      <path className={K.vector} d="M38 42 Q46 32 58 36" />
    </>
  ),
};

/** Trace Bitmap: the outline of a picture traced with lines or curves. */
export function tracePicture(kind: 'lines' | 'curves'): Picture {
  return {
    id: `trace-${kind}`,
    alt: t.trace[kind],
    draw: () => (
      <>
        <path
          className={K.model}
          d="M10 34 V26 H14 V18 H18 V14 H26 V10 H38 V14 H46 V18 H50 V26 H54 V34 Z"
        />
        <path
          className={K.vector}
          d={
            kind === 'lines'
              ? 'M10 34 L12 24 L18 15 L28 10 L36 10 L46 15 L52 24 L54 34 Z'
              : 'M10 34 C10 18 22 10 32 10 C42 10 54 18 54 34 Z'
          }
        />
      </>
    ),
  };
}

export type TextPosition = 'above' | 'on' | 'below';

/** Text on Curve: the text sitting above, on or below the curve. */
export function textOnCurvePicture(position: TextPosition): Picture {
  const dy = position === 'above' ? -8 : position === 'below' ? 8 : 0;
  return {
    id: `text-on-curve-${position}`,
    alt: t.textOnCurve[position],
    draw: () => (
      <>
        <path className={K.vector} d="M4 38 Q32 6 60 38" />
        {[
          [14, 27, -40],
          [24, 20, -22],
          [34, 18, 0],
          [44, 21, 22],
          [52, 28, 40],
        ].map(([x, y, angle]) => (
          <rect
            className={K.model}
            height={6}
            key={`${x}`}
            transform={`rotate(${angle} ${x} ${(y ?? 0) + dy})`}
            width={5}
            x={(x ?? 0) - 2.5}
            y={(y ?? 0) + dy - 3}
          />
        ))}
      </>
    ),
  };
}
