// How the GPU surface reads a sampled block of the heightfield: the block of
// `step` by `step` cells round each sampled cell, every `stride` cells when
// the block is large. The vertex shader and the CPU (hole walls) read the
// same cells so the surface edge and its walls meet.

/** Most samples along a side of a block. */
export const BLOCK_SAMPLES = 8;

export interface BlockRule {
  /** First and last cell offsets of the block round its sample. */
  lo: number;
  hi: number;
  /** Cells between the samples read. */
  stride: number;
}

export function blockRule(step: number): BlockRule {
  const s = Math.max(1, Math.floor(step));
  const lo = 0 - Math.floor(s / 2);
  return { lo, hi: lo + s - 1, stride: Math.floor((s + BLOCK_SAMPLES - 1) / BLOCK_SAMPLES) };
}

export interface BlockSample {
  /** Mean height of the kept cells, job mm, or NaN when every cell read is cut through. */
  mean: number;
  /** Share of the cells read that are cut through, 0 to 1. */
  hole: number;
}

/**
 * The block round cell (`cx`, `cy`) read through `read` (NaN for a cut
 * through cell; coordinates are clamped to the grid as the shader does).
 */
export function blockSample(
  read: (x: number, y: number) => number,
  width: number,
  height: number,
  cx: number,
  cy: number,
  rule: BlockRule,
): BlockSample {
  let sum = 0;
  let kept = 0;
  let n = 0;
  for (let dy = rule.lo; dy <= rule.hi; dy += rule.stride) {
    const y = Math.min(Math.max(cy + dy, 0), height - 1);
    for (let dx = rule.lo; dx <= rule.hi; dx += rule.stride) {
      const x = Math.min(Math.max(cx + dx, 0), width - 1);
      const z = read(x, y);
      n += 1;
      if (!Number.isNaN(z)) {
        sum += z;
        kept += 1;
      }
    }
  }
  return { mean: kept > 0 ? sum / kept : Number.NaN, hole: (n - kept) / n };
}
