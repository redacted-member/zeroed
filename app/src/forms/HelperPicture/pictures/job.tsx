import type { JSX } from 'react';

import type {
  FlipAxis,
  JobKind,
  RotaryAxis,
  RotaryZZero,
  XyOrigin,
  ZZero,
} from '../../../bridge/types';
import { helpersJobStrings } from '../../../lib/strings/helpersJob';
import { Dim, EndMill, K, Label, PathHead, type Picture } from '../kit';

// Job Setup and Material Setup pictures. The Material Setup summary in the
// Toolpaths panel draws two of them, loading this module on demand.

const t = helpersJobStrings.helpersJob;

interface Box {
  /** Front face left, right, top, bottom. */
  l: number;
  r: number;
  top: number;
  bottom: number;
  /** How far the back edges sit up and to the right. */
  dx: number;
  dy: number;
}

const BLOCK: Box = { l: 6, r: 40, top: 20, bottom: 38, dx: 12, dy: 10 };
const BOARD: Box = { l: 6, r: 40, top: 26, bottom: 34, dx: 12, dy: 10 };

/** The top, front and side faces of a block drawn in a simple oblique view. */
function Block({ box }: { box: Box }): JSX.Element {
  const { l, r, top, bottom, dx, dy } = box;
  return (
    <>
      <polygon
        className={K.material}
        points={`${l},${top} ${l + dx},${top - dy} ${r + dx},${top - dy} ${r},${top}`}
      />
      <rect className={K.material} height={bottom - top} width={r - l} x={l} y={top} />
      <polygon
        className={K.side}
        points={`${r},${top} ${r + dx},${top - dy} ${r + dx},${bottom - dy} ${r},${bottom}`}
      />
    </>
  );
}

const CYLINDER_X = (
  <>
    <path className={K.material} d="M12 14 H52 A4 10 0 0 1 52 34 H12 Z" />
    <ellipse className={K.side} cx={12} cy={24} rx={4} ry={10} />
    <line className={K.faint} x1={2} x2={62} y1={24} y2={24} />
  </>
);

const CYLINDER_Y = (
  <>
    <path className={K.material} d="M22 8 V40 A10 3 0 0 0 42 40 V8 Z" />
    <ellipse className={K.side} cx={32} cy={8} rx={10} ry={3} />
    <line className={K.faint} x1={32} x2={32} y1={2} y2={46} />
  </>
);

/** Job type: a board, a board turned over, or a cylinder. */
export function jobTypePicture(kind: JobKind): Picture {
  return {
    id: `job-type-${kind}`,
    alt: t.jobType[kind],
    draw: () =>
      kind === 'rotary' ? (
        CYLINDER_X
      ) : (
        <>
          <Block box={BOARD} />
          {kind === 'doubleSided' ? (
            <>
              <path className={K.path} d="M12 18 Q28 0 46 10" />
              <PathHead x1={40} x2={46} y1={6} y2={10} />
            </>
          ) : null}
        </>
      ),
  };
}

/** Job size: the block with its X, Y and Z sizes. */
export const jobSize: Picture = {
  id: 'job-size',
  alt: t.jobSize,
  draw: () => (
    <>
      <Block box={BLOCK} />
      <Dim x1={6} x2={40} y1={43} y2={43} />
      <Dim x1={43} x2={55} y1={42} y2={32} />
      <Dim x1={2} x2={2} y1={20} y2={38} />
      <Label x={23} y={48}>
        X
      </Label>
      <Label x={58} y={40}>
        Y
      </Label>
      <Label x={2} y={17}>
        Z
      </Label>
    </>
  ),
};

/** Z zero: the zero plane on the material surface or the machine bed. */
export function zZeroPicture(zZero: ZZero): Picture {
  const { l, r, top, bottom, dx, dy } = BLOCK;
  const y = zZero === 'top' ? top : bottom;
  return {
    id: `z-zero-${zZero}`,
    alt: t.zZero[zZero],
    draw: () => (
      <>
        {zZero === 'bottom' ? (
          <line className={K.line} x1={0} x2={62} y1={bottom + 2} y2={bottom + 2} />
        ) : null}
        <Block box={BLOCK} />
        <polygon
          className={K.plane}
          points={`${l},${y} ${l + dx},${y - dy} ${r + dx},${y - dy} ${r},${y}`}
        />
        <circle className={K.dot} cx={l} cy={y} r={2} />
      </>
    ),
  };
}

/** Rotary Z zero: on the cylinder surface or its center line. */
export function rotaryZZeroPicture(zZero: RotaryZZero): Picture {
  return {
    id: `rotary-z-zero-${zZero}`,
    alt: t.rotaryZZero[zZero],
    draw: () => (
      <>
        {CYLINDER_X}
        <line
          className={K.plane}
          x1={10}
          x2={56}
          y1={zZero === 'surface' ? 14 : 24}
          y2={zZero === 'surface' ? 14 : 24}
        />
      </>
    ),
  };
}

const DATUM: Record<XyOrigin, { x: number; y: number }> = {
  bottomLeft: { x: 8, y: 40 },
  bottomRight: { x: 56, y: 40 },
  topLeft: { x: 8, y: 8 },
  topRight: { x: 56, y: 8 },
  centre: { x: 32, y: 24 },
};

/** XY datum: the material from above with the datum at the picked point. */
export function xyDatumPicture(origin: XyOrigin): Picture {
  const { x, y } = DATUM[origin];
  return {
    id: `xy-datum-${origin}`,
    alt: t.xyDatum[origin],
    draw: () => (
      <>
        <rect className={K.material} height={32} width={48} x={8} y={8} />
        <line className={K.faint} x1={x - 7} x2={x + 7} y1={y} y2={y} />
        <line className={K.faint} x1={x} x2={x} y1={y - 7} y2={y + 7} />
        <circle className={K.dot} cx={x} cy={y} r={3} />
      </>
    ),
  };
}

/** Modeling resolution: a relief drawn coarse at the left, fine at the right. */
export const resolution: Picture = {
  id: 'modeling-resolution',
  alt: t.resolution,
  draw: () => (
    <>
      <path
        className={K.model}
        d="M4 40 V32 H10 V24 H16 V18 H24 V14 H32 Q44 14 50 24 Q55 32 60 40 Z"
      />
      <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
    </>
  ),
};

/** Rotary axis: the cylinder lying along X or along Y. */
export function rotaryAxisPicture(axis: RotaryAxis): Picture {
  return {
    id: `rotary-axis-${axis}`,
    alt: t.rotaryAxis[axis],
    draw: () => (
      <>
        {axis === 'x' ? CYLINDER_X : CYLINDER_Y}
        {axis === 'x' ? (
          <>
            <path className={K.path} d="M56 12 A4 12 0 0 1 56 36" />
            <PathHead x1={58} x2={56} y1={33} y2={36} />
          </>
        ) : (
          <>
            <path className={K.path} d="M18 44 A14 4 0 0 0 46 44" />
            <PathHead x1={43} x2={46} y1={45} y2={44} />
          </>
        )}
      </>
    ),
  };
}

/** Double sided flip: the board turning over left to right or top to bottom. */
export function flipPicture(flip: FlipAxis): Picture {
  return {
    id: `flip-${flip}`,
    alt: t.flip[flip],
    draw: () =>
      flip === 'horizontal' ? (
        <>
          <rect className={K.material} height={28} width={40} x={12} y={14} />
          <line className={K.faint} x1={32} x2={32} y1={8} y2={46} />
          <path className={K.path} d="M14 10 Q32 0 50 10" />
          <PathHead x1={45} x2={50} y1={7} y2={10} />
        </>
      ) : (
        <>
          <rect className={K.material} height={28} width={40} x={8} y={10} />
          <line className={K.faint} x1={2} x2={54} y1={24} y2={24} />
          <path className={K.path} d="M52 10 Q62 24 52 38" />
          <PathHead x1={56} x2={52} y1={34} y2={38} />
        </>
      ),
  };
}

/** Model position: the model inside the material, gaps above and below. */
export const modelPosition: Picture = {
  id: 'model-position',
  alt: t.modelPosition,
  draw: () => (
    <>
      <rect className={K.material} height={36} width={48} x={4} y={8} />
      <path className={K.model} d="M8 36 Q28 6 48 36 Z" />
      <line className={K.faint} x1={4} x2={52} y1={17} y2={17} />
      <Dim x1={58} x2={58} y1={8} y2={17} />
      <Dim x1={58} x2={58} y1={36} y2={44} />
    </>
  ),
};

/** Rapid Z gaps: clearance Z1 and plunge Z2 above the material surface. */
export const rapidGaps: Picture = {
  id: 'rapid-gaps',
  alt: t.rapidGaps,
  draw: () => (
    <>
      <rect className={K.material} height={14} width={44} x={4} y={32} />
      <EndMill width={8} x={20} y={22} />
      <line className={K.faint} x1={4} x2={52} y1={6} y2={6} />
      <line className={K.faint} x1={14} x2={52} y1={22} y2={22} />
      <Dim x1={44} x2={44} y1={6} y2={32} />
      <Dim x1={54} x2={54} y1={22} y2={32} />
      <Label x={38} y={14}>
        Z1
      </Label>
      <Label x={60} y={20}>
        Z2
      </Label>
    </>
  ),
};

/** Home position: the tool parked above the job. */
export const home: Picture = {
  id: 'home-position',
  alt: t.home,
  draw: () => (
    <>
      <rect className={K.material} height={12} width={52} x={8} y={34} />
      <EndMill width={8} x={12} y={24} />
      <circle className={K.dot} cx={12} cy={24} r={2} />
      <path className={K.faint} d="M12 26 V34" />
      <circle className={K.mark} cx={8} cy={34} r={1.6} />
    </>
  ),
};
