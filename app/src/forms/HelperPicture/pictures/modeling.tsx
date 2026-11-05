import type { JSX } from 'react';

import type { Combine, SculptMode } from '../../../bridge/model';
import { strings } from '../../../lib/formStrings';
import { Dim, K, PathHead, type Picture } from '../kit';

// Modeling form pictures: cross sections of the model (amber) on the base
// line, rails and profiles as vectors (blue). viewBox 64 x 48, combine and
// sculpt icons 24 x 24.

const t = strings.helpers.modeling;

const COMBINE_DRAWINGS: Record<Combine, () => JSX.Element> = {
  add: () => (
    <>
      <rect className={K.model} height={6} width={20} x={2} y={16} />
      <path className={K.model} d="M6 16 Q12 2 18 16 Z" />
    </>
  ),
  subtract: () => (
    <>
      <path className={K.model} d="M2 10 H6 Q12 22 18 10 H22 V22 H2 Z" />
    </>
  ),
  mergeHigh: () => (
    <>
      <path className={K.model} d="M2 20 Q8 4 14 20 Z" />
      <path className={K.model} d="M10 20 Q16 8 22 20 Z" />
      <path className={K.line} d="M2 20 Q8 4 12.5 15 Q16 8 22 20" />
    </>
  ),
  mergeLow: () => (
    <>
      <path className={K.model} d="M2 20 Q8 4 14 20 Z" />
      <path className={K.model} d="M10 20 Q16 8 22 20 Z" />
      <path className={K.line} d="M10 20 Q11 16 12.5 15 Q13 16 14 20" />
    </>
  ),
  multiply: () => (
    <>
      <path className={K.model} d="M2 20 Q12 4 22 20 Z" />
      <path className={K.line} d="M5 14 Q8 12 11 14 T17 14 T21 14" />
    </>
  ),
};

/** Combine modes: an icon beside each radio. */
export function combinePicture(mode: Combine): Picture {
  return {
    id: `combine-${mode}`,
    alt: t.combine[mode],
    viewBox: '0 0 24 24',
    draw: COMBINE_DRAWINGS[mode],
  };
}

export type ShapeTool = 'round' | 'flat' | 'angled' | 'smooth' | 'custom';

const SHAPE_SECTIONS: Record<ShapeTool, string> = {
  round: 'M6 40 Q6 8 32 8 Q58 8 58 40 Z',
  flat: 'M8 40 V14 H56 V40 Z',
  angled: 'M6 40 L32 8 L58 40 Z',
  smooth: 'M6 40 Q8 14 20 14 H44 Q56 14 58 40 Z',
  custom: 'M6 40 V30 Q14 30 16 22 Q18 12 28 12 L34 18 L40 12 Q52 12 58 40 Z',
};

/** Create Shape: the cross section each shape tool makes from a vector. */
export function shapeProfilePicture(tool: ShapeTool): Picture {
  return {
    id: `shape-${tool}`,
    alt: t.shapeProfile[tool],
    draw: () => (
      <>
        <path className={K.model} d={SHAPE_SECTIONS[tool]} />
        <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
        {tool === 'custom' ? null : <line className={K.vector} x1={6} x2={58} y1={44} y2={44} />}
      </>
    ),
  };
}

/** Create Shape heights: the shape on its base, its height scaled. */
export const shapeHeights: Picture = {
  id: 'shape-heights',
  alt: t.heights,
  draw: () => (
    <>
      <rect className={K.model} height={8} width={44} x={4} y={32} />
      <path className={K.model} d="M4 32 Q4 10 26 10 Q48 10 48 32 Z" />
      <line className={K.line} x1={2} x2={52} y1={40} y2={40} />
      <Dim x1={56} x2={56} y1={32} y2={40} />
      <Dim x1={56} x2={56} y1={10} y2={32} />
    </>
  ),
};

/** Turn and Spin: a profile swept round a rail, or spun round its center. */
export function turnSpinPicture(mode: 'turn' | 'spin'): Picture {
  return {
    id: `turn-spin-${mode}`,
    alt: t.turnSpin[mode],
    draw: () =>
      mode === 'turn' ? (
        <>
          <ellipse className={K.model} cx={32} cy={28} rx={26} ry={12} />
          <ellipse className={K.cut} cx={32} cy={28} rx={14} ry={5} />
          <ellipse className={K.vector} cx={32} cy={28} rx={20} ry={8.5} />
          <path className={K.vector} d="M52 28 Q52 14 56 12" />
        </>
      ) : (
        <>
          <path className={K.model} d="M8 40 Q10 12 32 10 Q54 12 56 40 Z" />
          <line className={K.faint} x1={32} x2={32} y1={4} y2={44} />
          <path className={K.vector} d="M32 10 Q50 12 56 40" />
          <path className={K.path} d="M22 6 A10 3 0 1 0 42 6" />
          <PathHead x1={39} x2={42} y1={8} y2={6} />
        </>
      ),
  };
}

/** Extrude and Weave: a strand along a rail, passing over and under when woven. */
export function weavePicture(weave: boolean): Picture {
  return {
    id: weave ? 'weave-on' : 'weave-off',
    alt: weave ? t.weaveOn : t.weaveOff,
    draw: () =>
      weave ? (
        <>
          <rect className={K.model} height={40} width={8} x={42} y={4} />
          <path className={K.model} d="M4 30 Q18 6 32 24 T60 18 L60 26 Q46 30 32 32 T4 38 Z" />
          <rect className={K.model} height={40} width={8} x={14} y={4} />
        </>
      ) : (
        <>
          <path className={K.model} d="M4 30 Q18 6 32 24 T60 18 L60 26 Q46 30 32 32 T4 38 Z" />
          <path className={K.vector} d="M4 34 Q18 12 32 28 T60 22" />
        </>
      ),
  };
}

/** Two Rail Sweep: cross sections between two rails. */
export const sweep: Picture = {
  id: 'two-rail-sweep',
  alt: t.sweep,
  draw: () => (
    <>
      <path className={K.model} d="M6 10 Q32 2 58 10 L58 38 Q32 46 6 38 Z" />
      <path className={K.vector} d="M6 10 Q32 2 58 10" />
      <path className={K.vector} d="M6 38 Q32 46 58 38" />
      <path className={K.line} d="M14 9 Q8 24 14 39 M32 6 Q24 24 32 42 M50 9 Q44 24 50 39" />
    </>
  ),
};

const SCULPT_DRAWINGS: Record<SculptMode, () => JSX.Element> = {
  normal: () => (
    <>
      <path className={K.model} d="M2 22 V14 Q7 8 12 14 T22 14 V22 Z" />
      <path className={K.line} d="M4 6 H20" />
    </>
  ),
  raise: () => (
    <>
      <path className={K.model} d="M2 22 V16 Q12 6 22 16 V22 Z" />
      <path className={K.line} d="M12 12 V2" />
      <PathHead x1={12} x2={12} y1={6} y2={2} />
    </>
  ),
  lower: () => (
    <>
      <path className={K.model} d="M2 22 V12 Q12 20 22 12 V22 Z" />
      <path className={K.line} d="M12 2 V12" />
      <PathHead x1={12} x2={12} y1={8} y2={12} />
    </>
  ),
};

/** Sculpt: an icon beside each brush mode. */
export function sculptPicture(mode: SculptMode): Picture {
  return {
    id: `sculpt-${mode}`,
    alt: t.sculpt[mode],
    viewBox: '0 0 24 24',
    draw: SCULPT_DRAWINGS[mode],
  };
}

/** Add Draft: the relief walls leaning at the draft angle. */
export const draft: Picture = {
  id: 'draft',
  alt: t.draft,
  draw: () => (
    <>
      <path className={K.faint} d="M14 40 V12 H50 V40" />
      <path className={K.model} d="M8 40 L16 12 H48 L56 40 Z" />
      <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
      <path className={K.dim} d="M14 26 A14 14 0 0 0 12 26" />
    </>
  ),
};

/** Replace Below: the model cut level at the height. */
export const replaceBelow: Picture = {
  id: 'replace-below',
  alt: t.replaceBelow,
  draw: () => (
    <>
      <path className={K.model} d="M4 40 V30 Q14 30 18 20 Q32 2 46 20 Q50 30 60 30 V40 Z" />
      <line className={K.plane} x1={2} x2={62} y1={30} y2={30} />
      <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
    </>
  ),
};

/** Offset model: the model grown by the distance all round. */
export const offsetModel: Picture = {
  id: 'offset-model',
  alt: t.offsetModel,
  draw: () => (
    <>
      <path className={K.faint} d="M4 40 Q4 4 32 4 Q60 4 60 40" />
      <path className={K.model} d="M12 40 Q12 12 32 12 Q52 12 52 40 Z" />
      <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
      <Dim x1={32} x2={32} y1={4} y2={12} />
    </>
  ),
};

/** Scale Model Height: the model squashed to the height. */
export const scaleHeight: Picture = {
  id: 'scale-height',
  alt: t.scaleHeight,
  draw: () => (
    <>
      <path className={K.faint} d="M6 40 Q6 6 30 6 Q54 6 54 40" />
      <path className={K.model} d="M6 40 Q6 22 30 22 Q54 22 54 40 Z" />
      <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
      <Dim x1={58} x2={58} y1={22} y2={40} />
    </>
  ),
};

/** Slice Model: the model cut into layers. */
export const slice: Picture = {
  id: 'slice-model',
  alt: t.slice,
  draw: () => (
    <>
      <path className={K.model} d="M6 40 Q6 6 32 6 Q58 6 58 40 Z" />
      <path className={K.plane} d="M6 31 H58 M8 22 H56 M14 13 H50" />
      <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
    </>
  ),
};

/** Position and Orientate Model: the zero plane through the model. */
export const zeroPlane: Picture = {
  id: 'zero-plane',
  alt: t.zeroPlane,
  draw: () => (
    <>
      <path className={K.model} d="M10 24 Q10 6 32 6 Q54 6 54 24 Q54 42 32 42 Q10 42 10 24 Z" />
      <line className={K.plane} x1={2} x2={62} y1={24} y2={24} />
    </>
  ),
};

/** Rotary import: the model bent round the axis, or flat over it. */
export function rotaryWrapPicture(on: boolean): Picture {
  return {
    id: on ? 'rotary-wrap-on' : 'rotary-wrap-off',
    alt: on ? t.rotaryWrap.on : t.rotaryWrap.off,
    draw: () => (
      <>
        <path className={K.material} d="M12 18 H52 A4 10 0 0 1 52 38 H12 Z" />
        <ellipse className={K.side} cx={12} cy={28} rx={4} ry={10} />
        <line className={K.faint} x1={2} x2={62} y1={28} y2={28} />
        {on ? (
          <path className={K.model} d="M20 18 Q32 8 44 18 Z" />
        ) : (
          <path className={K.model} d="M18 18 Q32 4 46 18 V10 H18 Z" />
        )}
      </>
    ),
  };
}
