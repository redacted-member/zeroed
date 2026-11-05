// Pure form state for Edit Picture: tone sliders, invert, greyscale and a
// faded border for the selected bitmap.

export type PictureBorder = 'none' | 'rectangle' | 'oval';

export interface PictureState {
  /** -100 to 100. */
  contrast: number;
  /** -100 to 100. */
  brightness: number;
  /** 0.2 to 5, 1 leaves the picture alone. */
  gamma: number;
  invert: boolean;
  greyscale: boolean;
  border: PictureBorder;
  /** Border fade, percent of the smaller side, 0 to 50. */
  fade: number;
}

export interface EditPictureArgs extends PictureState {
  id: number;
}

export function initialPictureState(): PictureState {
  return {
    contrast: 0,
    brightness: 0,
    gamma: 1,
    invert: false,
    greyscale: false,
    border: 'none',
    fade: 10,
  };
}

/** True when applying the settings would not change the picture. */
export function isNeutral(state: PictureState): boolean {
  return (
    state.contrast === 0 &&
    state.brightness === 0 &&
    state.gamma === 1 &&
    !state.invert &&
    !state.greyscale &&
    state.border === 'none'
  );
}

/** The kernel request, or null without a bitmap or with nothing to change. */
export function pictureRequest(
  state: PictureState,
  bitmapId: number | null,
): EditPictureArgs | null {
  if (bitmapId === null || isNeutral(state)) {
    return null;
  }
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  return {
    id: bitmapId,
    contrast: clamp(state.contrast, -100, 100),
    brightness: clamp(state.brightness, -100, 100),
    gamma: clamp(state.gamma, 0.1, 10),
    invert: state.invert,
    greyscale: state.greyscale,
    border: state.border,
    fade: clamp(state.fade, 0, 50),
  };
}

/** One channel, 0 to 1, through contrast, brightness, gamma and invert, as
 * the kernel's Edit Picture maps it. */
export function toneOf(state: PictureState, value: number): number {
  const factor = state.contrast >= 0 ? 1 + (state.contrast / 100) * 4 : 1 + state.contrast / 100;
  let v = (value - 0.5) * factor + 0.5 + state.brightness / 100;
  v = Math.pow(Math.min(1, Math.max(0, v)), 1 / state.gamma);
  return state.invert ? 1 - v : v;
}

function smoothStep(t: number): number {
  const c = Math.min(1, Math.max(0, t));
  return c * c * (3 - 2 * c);
}

/** How much of the picture shows at pixel centre (x, y) of a w by h picture. */
export function borderWeight(
  state: PictureState,
  x: number,
  y: number,
  w: number,
  h: number,
): number {
  const fade = (state.fade / 100) * Math.min(w, h);
  if (state.border === 'rectangle') {
    if (fade <= 0) {
      return 1;
    }
    return smoothStep(Math.min(x, w - x, y, h - y) / fade);
  }
  if (state.border === 'oval') {
    const [rx, ry] = [w / 2, h / 2];
    const r = Math.hypot((x - rx) / rx, (y - ry) / ry);
    const inside = (1 - r) * Math.min(rx, ry);
    if (fade <= 0) {
      return inside >= 0 ? 1 : 0;
    }
    return smoothStep(inside / fade);
  }
  return 1;
}

/** The picture as Apply would leave it, for the live preview: RGBA pixels,
 * row by row, edited in place. */
export function editPixels(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  state: PictureState,
): Uint8ClampedArray {
  const lut = new Uint8ClampedArray(256);
  for (let i = 0; i < 256; i += 1) {
    lut[i] = Math.round(toneOf(state, i / 255) * 255);
  }
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const at = (y * width + x) * 4;
      let r = pixels[at] ?? 0;
      let g = pixels[at + 1] ?? 0;
      let b = pixels[at + 2] ?? 0;
      if (state.greyscale) {
        const luma = Math.round(0.299 * r + 0.587 * g + 0.114 * b);
        [r, g, b] = [luma, luma, luma];
      }
      const weight = borderWeight(state, x + 0.5, y + 0.5, width, height);
      const blend = (c: number) =>
        Math.round(((lut[c] ?? 0) / 255) * weight * 255 + (1 - weight) * 255);
      pixels[at] = blend(r);
      pixels[at + 1] = blend(g);
      pixels[at + 2] = blend(b);
    }
  }
  return pixels;
}
