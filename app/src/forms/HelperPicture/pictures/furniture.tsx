import { strings } from '../../../lib/formStrings';
import { Dim, K, Label, PathArrow, type Picture } from '../kit';

// Furniture Designer, Joint Creator and Assembly View pictures: panels as
// material, the seen faces of a box in the material's side tone, sizes as
// dimensions with the letter the fields use. viewBox 64 x 48.

const t = strings.helpers.furniture;

/** The overall size: a cabinet seen from the front corner, width, depth and height. */
export const furnitureSize: Picture = {
  id: 'furniture-size',
  alt: t.size,
  draw: () => (
    <>
      <path className={K.side} d="M34 10 L46 4 V32 L34 38 Z" />
      <path className={K.side} d="M10 10 L22 4 H46 L34 10 Z" />
      <rect className={K.material} height={28} width={24} x={10} y={10} />
      <line className={K.line} x1={14} x2={30} y1={24} y2={24} />
      <Dim x1={10} x2={34} y1={43} y2={43} />
      <Label x={22} y={41}>
        W
      </Label>
      <Dim x1={36} x2={48} y1={41} y2={35} />
      <Label x={46} y={42}>
        D
      </Label>
      <Dim x1={52} x2={52} y1={4} y2={32} />
      <Label x={58} y={20}>
        H
      </Label>
    </>
  ),
};

/** Stock thickness: a board seen end on, its thickness across. */
export const furnitureThickness: Picture = {
  id: 'furniture-thickness',
  alt: t.thickness,
  draw: () => (
    <>
      <rect className={K.material} height={8} width={44} x={6} y={20} />
      <path className={K.side} d="M50 20 L58 14 V22 L50 28 Z" />
      <path className={K.side} d="M6 20 L14 14 H58 L50 20 Z" />
      <Dim x1={4} x2={4} y1={20} y2={28} />
      <Label x={10} y={40}>
        T
      </Label>
    </>
  ),
};

/** Fit: a tenon in its slot seen from above, the clearance on each side. */
export const jointFit: Picture = {
  id: 'joint-fit',
  alt: t.fit,
  draw: () => (
    <>
      <path className={K.material} d="M4 28 H24 V40 H40 V28 H60 V44 H4 Z" />
      <rect className={K.model} height={30} width={14} x={25} y={6} />
      <line className={K.dim} x1={24} x2={24} y1={24} y2={44} />
      <line className={K.dim} x1={40} x2={40} y1={24} y2={44} />
      <Label x={15} y={24}>
        C
      </Label>
      <Label x={49} y={24}>
        C
      </Label>
    </>
  ),
};

/** Explode: the pieces drawn apart along the way they go together. */
export const assemblyExplode: Picture = {
  id: 'assembly-explode',
  alt: t.explode,
  draw: () => (
    <>
      <rect className={K.material} height={8} width={48} x={8} y={36} />
      <rect className={K.model} height={20} width={8} x={14} y={6} />
      <rect className={K.model} height={20} width={8} x={42} y={6} />
      <rect className={K.faint} height={8} width={8} x={14} y={28} />
      <rect className={K.faint} height={8} width={8} x={42} y={28} />
      <PathArrow x1={32} x2={32} y1={34} y2={10} />
    </>
  ),
};

/** Verify Fit: the gap between two pieces against the target, give or take the tolerance. */
export const assemblyFit: Picture = {
  id: 'assembly-fit',
  alt: t.verifyFit,
  draw: () => (
    <>
      <path className={K.material} d="M4 20 H26 V8 H38 V20 H60 V40 H4 Z" />
      <rect className={K.model} height={26} width={8} x={28} y={2} />
      <Dim x1={26} x2={28} y1={24} y2={24} />
      <Label x={20} y={32}>
        G
      </Label>
    </>
  ),
};
