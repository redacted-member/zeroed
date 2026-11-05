// The icon spec format (ToolIcons.tsx draws it): shapes separated by ';',
// each an optional flag set and ':' before the shape. Lower case flags set
// the line style or a surface fill, upper case flags a color role. A shape
// with no role draws in the neutral text color, so most of an icon stays
// neutral and a role marks the one or two parts that matter, the way tool
// families are told apart at a glance: vectors blue, nodes and selection
// orange, relief amber on a gold surface, toolpaths purple on wood, what an
// edit makes or removes red, what it confirms green.
//
// Every flag is a class of ToolIcons.scss; the role classes set `color`
// from a token of styles/_tokens.scss, so strokes, fills and tints of the
// shape take it, and a disabled or highlighted control turns every role
// back into its own text color (the tool-icon-mono mixin).

/** A lower case flag: how a shape is stroked or filled. */
export interface StyleFlag {
  kind: 'style';
  className: string;
}

/** A color role (upper case flag), or a surface fill (lower case). */
export interface ColorFlag {
  kind: 'role' | 'surface';
  className: string;
  /** The custom property of styles/_tokens.scss that holds its color. */
  token: string;
}

export type IconFlag = StyleFlag | ColorFlag;

export const ICON_FLAGS: Readonly<Record<string, IconFlag>> = {
  t: { kind: 'style', className: 'tool-icon__thin' },
  d: { kind: 'style', className: 'tool-icon__dash' },
  f: { kind: 'style', className: 'tool-icon__fill' },
  n: { kind: 'style', className: 'tool-icon__tint' },
  s: { kind: 'surface', className: 'tool-icon__surface', token: 'icon-model-fill' },
  w: { kind: 'surface', className: 'tool-icon__wood', token: 'icon-material-fill' },
  V: { kind: 'role', className: 'tool-icon__vector', token: 'icon-vector' },
  S: { kind: 'role', className: 'tool-icon__node', token: 'icon-node' },
  M: { kind: 'role', className: 'tool-icon__model', token: 'icon-model' },
  P: { kind: 'role', className: 'tool-icon__toolpath', token: 'icon-toolpath' },
  R: { kind: 'role', className: 'tool-icon__result', token: 'icon-result' },
  G: { kind: 'role', className: 'tool-icon__ok', token: 'icon-ok' },
  A: { kind: 'role', className: 'tool-icon__warning', token: 'icon-warning' },
};

/** One shape of a spec: its flag letters and what follows the ':'. */
export interface IconShape {
  flags: string;
  body: string;
}

/** Splits a spec into its shapes. */
export function iconShapes(spec: string): IconShape[] {
  return spec.split(';').map((part) => {
    const colon = part.indexOf(':');
    return colon < 0
      ? { flags: '', body: part }
      : { flags: part.slice(0, colon), body: part.slice(colon + 1) };
  });
}

/** The classes of a shape's flags, or undefined for none. Unknown letters are skipped. */
export function flagClasses(flags: string): string | undefined {
  const names = [...flags].flatMap((letter) => {
    const flag = ICON_FLAGS[letter];
    return flag === undefined ? [] : [flag.className];
  });
  return names.length === 0 ? undefined : names.join(' ');
}

/** The flag letters of a spec that the format does not know. */
export function unknownFlags(spec: string): string[] {
  return iconShapes(spec).flatMap((shape) =>
    [...shape.flags].filter((letter) => ICON_FLAGS[letter] === undefined),
  );
}

/** The color roles (upper case flags) a spec uses, each once, in order. */
export function iconRoles(spec: string): string[] {
  const roles = iconShapes(spec).flatMap((shape) =>
    [...shape.flags].filter((letter) => ICON_FLAGS[letter]?.kind === 'role'),
  );
  return [...new Set(roles)];
}
