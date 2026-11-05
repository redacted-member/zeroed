// Where a component lands in the Assembly View: dragged from the Component
// Tree and let go over the 3D view, or placed with a click after Place in
// the Add Component list. The view finds the point under the pointer and
// the piece it is on (else the ground, assembly Z 0); the kernel
// (assembly.dropPlace) sets the component flush against that face, clear
// of every piece. The drag runs on pointer events, as the Clipart tab's
// does: the 3D view registers its surface with a function that turns a
// screen point into the landing under it.

import type { DropLanding, PieceRef, PieceRow, Triple } from '../../bridge/assembly';
import { piecesBox } from './pieceGizmo';

/** Turns a screen point (client pixels) into the landing under it, or
 * null when nothing there can take a piece. */
export type PieceDropResolver = (clientX: number, clientY: number) => DropLanding | null;

const targets = new Map<Element, PieceDropResolver>();

/** Makes `element` a place pieces can be dropped; answers the undo. */
export function registerPieceDrop(element: Element, resolve: PieceDropResolver): () => void {
  targets.set(element, resolve);
  return () => {
    if (targets.get(element) === resolve) {
      targets.delete(element);
    }
  };
}

/** Where a piece let go at a screen point lands, or null when no view
 * that takes pieces is under it. */
export function pieceDropAt(
  clientX: number,
  clientY: number,
  hit: (x: number, y: number) => Element | null = (x, y) => document.elementFromPoint(x, y),
  registry: ReadonlyMap<Element, PieceDropResolver> = targets,
): DropLanding | null {
  for (let at = hit(clientX, clientY); at !== null; at = at.parentElement) {
    const resolve = registry.get(at);
    if (resolve !== undefined) {
      return resolve(clientX, clientY);
    }
  }
  return null;
}

/** Where a ray (assembly space) meets the ground, Z 0, in front of its
 * origin; null when it runs level with or away from the ground. */
export function groundPoint(ray: { origin: Point3; direction: Point3 }): Triple | null {
  const [ox, oy, oz] = ray.origin;
  const [dx, dy, dz] = ray.direction;
  if (Math.abs(dz) < 1e-9) {
    return null;
  }
  const t = -oz / dz;
  return t > 0 ? [ox + dx * t, oy + dy * t, 0] : null;
}

type Point3 = readonly [number, number, number];

/** The two picks the 3D view offers, normalised device coordinates. */
export interface PiecePicker {
  pickPiece: (ndcX: number, ndcY: number) => { key: string; point: Point3 } | null;
  jobRay: (ndcX: number, ndcY: number) => { origin: Point3; direction: Point3 };
}

/**
 * The landing under a canvas point: the point on the piece the pointer is
 * over (not `skip`, the piece being moved) with that piece (`refOf` names
 * it from its key), else the point on the ground.
 */
export function landingPoint(
  picker: PiecePicker,
  ndcX: number,
  ndcY: number,
  skip: string | null,
  refOf: (key: string) => PieceRef | null,
): DropLanding | null {
  const hit = picker.pickPiece(ndcX, ndcY);
  if (hit !== null && hit.key !== skip) {
    const onto = refOf(hit.key);
    if (onto !== null) {
      return { point: [hit.point[0], hit.point[1], hit.point[2]], onto };
    }
  }
  const ground = groundPoint(picker.jobRay(ndcX, ndcY));
  return ground === null ? null : { point: ground, onto: null };
}

/**
 * Where a piece's middle goes so that the middle of the underside of its
 * box (as turned) sits on `at`: it keeps its turns and rests there. The
 * ghost stands so until the kernel answers where it lands.
 */
export function restingPosition(
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>,
  at: Triple,
): Triple {
  const box = piecesBox([piece]);
  if (box === null) {
    return at;
  }
  const under: Triple = [(box.min[0] + box.max[0]) / 2, (box.min[1] + box.max[1]) / 2, box.min[2]];
  return [
    piece.position[0] + at[0] - under[0],
    piece.position[1] + at[1] - under[1],
    piece.position[2] + at[2] - under[2],
  ];
}
