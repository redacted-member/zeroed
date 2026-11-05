// What a copy of the selection moved by a matrix (Ctrl drag, Duplicate,
// the Rotate and Mirror copies) takes: every selected object of any kind
// (paths, text, bitmaps, groups whole) and the selected components, copied
// by the kernel in one step so a shape copied with its vectors is made
// from their copies.

export interface CopyRequest {
  /** Top level entities to copy. */
  vectors: number[];
  /** Relief components to copy. */
  components: number[];
}

/** The objects to copy: `ids` alone when given (a form copying some of the
 * selection), otherwise the selected entities with the selected
 * components. Null when there is nothing to copy. */
export function copyRequest(
  selection: readonly number[],
  picked: readonly number[],
  ids?: readonly number[],
): CopyRequest | null {
  const vectors = [...new Set(ids ?? selection)];
  const components = ids === undefined ? [...new Set(picked)] : [];
  return vectors.length === 0 && components.length === 0 ? null : { vectors, components };
}
