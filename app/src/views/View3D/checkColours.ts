// Colors the preview check paints the surface with and its legend shows:
// gouges red, leftover blue, undercuts violet.

export const CHECK_COLOURS = {
  gouge: '#d93636',
  leftover: '#2f6fe0',
  undercut: '#9a45d8',
} as const;

/** Colors of the issue markers by severity. */
export const SEVERITY_COLOURS = {
  high: '#e0332d',
  medium: '#f0a020',
  low: '#3a8ee6',
} as const;
