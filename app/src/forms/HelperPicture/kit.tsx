import type { JSX } from 'react';

/** One helper drawing: its id (data-helper, for tests) and SVG content. */
export interface Picture {
  id: string;
  /** What the picture shows, read out in place of it. */
  alt: string;
  /** Defaults to 0 0 64 48. */
  viewBox?: string;
  draw: () => JSX.Element;
}

/** Class names of the drawing roles, styled in HelperPicture.scss. */
export const K = {
  line: 'helper-picture__line',
  faint: 'helper-picture__faint',
  material: 'helper-picture__material',
  side: 'helper-picture__material-side',
  model: 'helper-picture__model',
  cut: 'helper-picture__cut',
  tool: 'helper-picture__tool',
  vector: 'helper-picture__vector',
  path: 'helper-picture__path',
  pathHead: 'helper-picture__path-head',
  dim: 'helper-picture__dim',
  dimHead: 'helper-picture__dim-head',
  dot: 'helper-picture__dot',
  mark: 'helper-picture__mark',
  plane: 'helper-picture__plane',
  label: 'helper-picture__label',
} as const;

/** A filled triangle pointing from (x1, y1) towards (x2, y2), tip at (x2, y2). */
export function headPoints(x1: number, y1: number, x2: number, y2: number, size = 3.2): string {
  const angle = Math.atan2(y2 - y1, x2 - x1);
  const spread = 0.45;
  const ax = x2 - size * Math.cos(angle - spread);
  const ay = y2 - size * Math.sin(angle - spread);
  const bx = x2 - size * Math.cos(angle + spread);
  const by = y2 - size * Math.sin(angle + spread);
  const round = (value: number) => Math.round(value * 100) / 100;
  return `${round(x2)},${round(y2)} ${round(ax)},${round(ay)} ${round(bx)},${round(by)}`;
}

interface LineProps {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** A dimension: a thin line with a head at both ends. */
export function Dim({ x1, y1, x2, y2 }: LineProps): JSX.Element {
  return (
    <g>
      <line className={K.dim} x1={x1} x2={x2} y1={y1} y2={y2} />
      <polygon className={K.dimHead} points={headPoints(x1, y1, x2, y2)} />
      <polygon className={K.dimHead} points={headPoints(x2, y2, x1, y1)} />
    </g>
  );
}

/** A toolpath arrow: a line with a head at its end. */
export function PathArrow({ x1, y1, x2, y2 }: LineProps): JSX.Element {
  return (
    <g>
      <line className={K.path} x1={x1} x2={x2} y1={y1} y2={y2} />
      <polygon className={K.pathHead} points={headPoints(x1, y1, x2, y2)} />
    </g>
  );
}

/** A head alone at (x2, y2) pointing away from (x1, y1), for curved paths. */
export function PathHead({ x1, y1, x2, y2 }: LineProps): JSX.Element {
  return <polygon className={K.pathHead} points={headPoints(x1, y1, x2, y2)} />;
}

interface LabelProps {
  x: number;
  y: number;
  children: string;
  anchor?: 'start' | 'middle' | 'end';
}

/** A one letter dimension label (D, C, F, A), as the field labels name them. */
export function Label({ x, y, children, anchor = 'middle' }: LabelProps): JSX.Element {
  return (
    <text aria-hidden="true" className={K.label} textAnchor={anchor} x={x} y={y}>
      {children}
    </text>
  );
}

/** A side view of the material: a slab with its top at `top`. */
export function Slab({ top, bottom = 44 }: { top: number; bottom?: number }): JSX.Element {
  return <rect className={K.material} height={bottom - top} width={56} x={4} y={top} />;
}

/** An end mill seen from the side, its tip at (x, y). */
export function EndMill({
  x,
  y,
  width = 8,
}: {
  x: number;
  y: number;
  width?: number;
}): JSX.Element {
  const half = width / 2;
  return (
    <g>
      <rect className={K.tool} height={14} width={width} x={x - half} y={y - 14} />
      <rect className={K.tool} height={8} width={width - 3} x={x - half + 1.5} y={y - 22} />
    </g>
  );
}

/** A V bit seen from the side, its tip at (x, y). */
export function VBit({
  x,
  y,
  half = 6,
  height = 8,
}: {
  x: number;
  y: number;
  half?: number;
  height?: number;
}): JSX.Element {
  return (
    <g>
      <polygon
        className={K.tool}
        points={`${x},${y} ${x - half},${y - height} ${x + half},${y - height}`}
      />
      <rect
        className={K.tool}
        height={10}
        width={half * 2 - 4}
        x={x - half + 2}
        y={y - height - 10}
      />
    </g>
  );
}
