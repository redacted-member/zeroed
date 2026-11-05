// The part of a data texture to re-upload when only a rectangle of cells
// changed: the rectangle clipped to the texture.

/** A rectangle of cells: first column and row, and its size. */
export interface CellRect {
  x0: number;
  y0: number;
  width: number;
  height: number;
}

/** `rect` clipped to a texture `width` by `height` texels, or null when none of it is inside. */
export function clipRect(rect: CellRect, width: number, height: number): CellRect | null {
  const x0 = Math.max(0, rect.x0);
  const y0 = Math.max(0, rect.y0);
  const x1 = Math.min(width, rect.x0 + rect.width);
  const y1 = Math.min(height, rect.y0 + rect.height);
  if (x1 <= x0 || y1 <= y0) {
    return null;
  }
  return { x0, y0, width: x1 - x0, height: y1 - y0 };
}
