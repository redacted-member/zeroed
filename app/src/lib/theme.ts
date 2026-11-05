// Canvas renderers cannot use CSS custom properties directly, so they read the
// resolved token values once and again whenever the colour scheme changes.

export interface CanvasPalette {
  background: string;
  grid: string;
  gridMajor: string;
  jobFill: string;
  jobBorder: string;
  rulerBackground: string;
  rulerLine: string;
  rulerText: string;
  datum: string;
  selection: string;
  hover: string;
  handle: string;
  handleFill: string;
  guide: string;
  snap: string;
  rubberBand: string;
  rubberBandFill: string;
  node: string;
  nodeFill: string;
  nodeSmooth: string;
  controlHandle: string;
  openVector: string;
  toolpathCut: string;
  toolpathRapid: string;
  /** Fill of a solid 2D toolpath preview. */
  toolpathSolid: string;
  toolpathPlunge: string;
  toolpathRetract: string;
  toolpathJoin: string;
  tileLine: string;
  tileOverlap: string;
  /** First and second rail of a Two-Rail Sweep. */
  railFirst: string;
  railSecond: string;
}

export interface ScenePalette {
  backgroundTop: string;
  backgroundBottom: string;
  material: string;
  grid: string;
}

function token(styles: CSSStyleDeclaration, name: string, fallback: string): string {
  const value = styles.getPropertyValue(name).trim();
  return value === '' ? fallback : value;
}

/** Reads the 2D view palette from the document tokens. */
export function readCanvasPalette(element: Element): CanvasPalette {
  const styles = getComputedStyle(element);
  return {
    background: token(styles, '--canvas-bg', '#ffffff'),
    grid: token(styles, '--canvas-grid', '#e8e8e8'),
    gridMajor: token(styles, '--canvas-grid-major', '#d2d2d2'),
    jobFill: token(styles, '--canvas-job-fill', '#ffffff'),
    jobBorder: token(styles, '--canvas-job-border', '#6f6f6f'),
    rulerBackground: token(styles, '--canvas-ruler-bg', '#ebebeb'),
    rulerLine: token(styles, '--canvas-ruler-line', '#bdbdbd'),
    rulerText: token(styles, '--canvas-ruler-text', '#4a4a4a'),
    datum: token(styles, '--canvas-datum', '#1f6fd6'),
    selection: token(styles, '--canvas-selection', '#e01ec8'),
    hover: token(styles, '--canvas-hover', '#ff7f00'),
    handle: token(styles, '--canvas-handle', '#1f6fd6'),
    handleFill: token(styles, '--canvas-handle-fill', '#ffffff'),
    guide: token(styles, '--canvas-guide', '#00a5b8'),
    snap: token(styles, '--canvas-snap', '#d62828'),
    rubberBand: token(styles, '--canvas-rubber-band', '#1f6fd6'),
    rubberBandFill: token(styles, '--canvas-rubber-band-fill', 'rgba(31, 111, 214, 0.08)'),
    node: token(styles, '--canvas-node', '#1c1c1c'),
    nodeFill: token(styles, '--canvas-node-fill', '#ffffff'),
    nodeSmooth: token(styles, '--canvas-node-smooth', '#1f6fd6'),
    controlHandle: token(styles, '--canvas-control-handle', '#8a8a8a'),
    openVector: token(styles, '--canvas-open-vector', '#d62828'),
    toolpathCut: token(styles, '--canvas-toolpath-cut', '#1d4ed8'),
    toolpathRapid: token(styles, '--canvas-toolpath-rapid', '#d62828'),
    toolpathSolid: token(styles, '--canvas-toolpath-solid', '#5b8def'),
    toolpathPlunge: token(styles, '--canvas-toolpath-plunge', '#00a8c6'),
    toolpathRetract: token(styles, '--canvas-toolpath-retract', '#2e9e44'),
    toolpathJoin: token(styles, '--canvas-toolpath-join', '#8a3ffc'),
    tileLine: token(styles, '--canvas-tile-line', '#e0a800'),
    tileOverlap: token(styles, '--canvas-tile-overlap', 'rgba(230, 70, 70, 0.22)'),
    railFirst: token(styles, '--canvas-rail-first', '#d62828'),
    railSecond: token(styles, '--canvas-rail-second', '#2e9e44'),
  };
}

/** Reads the 3D view palette from the document tokens. */
export function readScenePalette(element: Element): ScenePalette {
  const styles = getComputedStyle(element);
  return {
    backgroundTop: token(styles, '--view3d-top', '#4c525a'),
    backgroundBottom: token(styles, '--view3d-bottom', '#23262b'),
    material: token(styles, '--view3d-material', '#bda684'),
    grid: token(styles, '--view3d-grid', '#5c636c'),
  };
}

/** The `#rrggbb` or `#rgb` colour as 0 to 255 channels, or null for anything else. */
function channels(colour: string): [number, number, number] | null {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(colour.trim());
  if (hex === null) {
    return null;
  }
  const digits = hex[1] ?? '';
  const full = digits.length === 3 ? [...digits].map((d) => d + d).join('') : digits;
  const value = parseInt(full, 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

function luminance([r, g, b]: [number, number, number]): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/**
 * A layer colour that stays readable on the background: the layer's own colour
 * unless its contrast against the background is below 3:1, then the fallback
 * (a black default layer on the dark canvas shows in the palette's node colour).
 */
export function legibleOn(colour: string, background: string, fallback: string): string {
  const fg = channels(colour);
  const bg = channels(background);
  if (fg === null || bg === null) {
    return colour;
  }
  const a = luminance(fg);
  const b = luminance(bg);
  const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  return ratio < 3 ? fallback : colour;
}
