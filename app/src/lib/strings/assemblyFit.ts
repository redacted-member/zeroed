// The Assembly View's Fit and Explode sections. Only the lazily loaded
// form and 3D view read these, so they are imported directly.

export const assemblyFitStrings = {
  fit: 'Fit',
  fitHint:
    'Measures every joint between the placed pieces on their exact cut edges: each toolpath swept by its tool. A fit (walls either side) is judged by its smallest gap.',
  target: 'Target clearance',
  tolerance: 'Tolerance (±)',
  verify: 'Verify Fit',
  verifying: 'Verifying...',
  badValue: 'Enter a number of zero or more.',
  noJoints: 'No pieces meet, so there are no joints.',
  summary: (joints: number, problems: number) =>
    `${joints === 1 ? '1 joint' : `${joints} joints`}, ${
      problems === 0 ? 'all fit' : problems === 1 ? '1 needs a look' : `${problems} need a look`
    }.`,
  status: {
    ok: 'OK',
    loose: 'Loose',
    tight: 'Tight',
    interference: 'Interference',
  },
  joint: (a: string, b: string) => `${a} / ${b}`,
  /** A side's gap: one value when it is the same all along. */
  gap: (side: string, min: string, max: string | null) =>
    max === null ? `${side} ${min}` : `${side} ${min} to ${max}`,
  rests: (side: string) => `${side} rests`,
  /** A round fit is named for its axis. */
  round: (axis: string) => `Round about ${axis}`,
  overlap: (depth: string, volume: string, unit: string) =>
    `Runs in ${depth} ${unit}, ${volume} ${unit}³ shared`,
  exact: 'Exact edges',
  grid: (cell: string, unit: string) => `Simulation grid, cell ${cell} ${unit}`,
  showJoint: (label: string) => `Show the joint ${label} in the 3D view`,
  showFit: 'Show Fit',
  includeHidden: 'Include hidden pieces',
  includeHiddenHint:
    'Checks every piece, hidden ones too. Clear it to check only the pieces the view shows.',
  explode: 'Explode',
  explodeHint: 'Moves the pieces apart out of their joints to see how they go together.',
  explodeNeedsPieces: 'Explode needs at least two pieces.',
  explodeValue: (percent: number) => `${percent}%`,
  explodeLoading: 'Working out the explode...',
  steps: {
    title: 'Explode Steps',
    hint: 'Named steps the slider plays in order, each moving its pieces a distance along a direction. With none, the slider moves each piece out of its joints.',
    newStep: 'New Step',
    pickFirst: 'Pick a piece in the list or the 3D view first.',
    name: 'Name',
    direction: 'Direction',
    alongJoints: 'Out of its joints',
    distance: 'Distance',
    pieces: 'Pieces',
    addPiece: 'Add Picked Piece',
    removePiece: (name: string) => `Take ${name} out of the step`,
    apply: 'Apply',
    up: 'Move Up',
    down: 'Move Down',
    remove: 'Delete Step',
    defaultName: (n: number) => `Step ${n}`,
    edit: (name: string) => `Step: ${name}`,
    undo: {
      add: 'Add Explode Step',
      edit: 'Edit Explode Step',
      remove: 'Delete Explode Step',
      move: 'Move Explode Step',
    },
  },
  section: 'Section',
  sectionOn: 'Cut the pieces with a plane',
  sectionAxis: 'Plane across',
  sectionOffset: 'Offset',
  sectionFlip: 'Flip (keep the other side)',
  measure: 'Measure',
  measureHint: 'Measure between two points on the pieces, snapped to their faces.',
  pickPoints: 'Pick Points',
  cancel: 'Cancel',
  pickFirst: 'Click a point on a piece.',
  pickSecond: 'Now click the point to measure to.',
  measuring: 'Measuring...',
  distance: (d: string, dx: string, dy: string, dz: string, unit: string) =>
    `Distance ${d} ${unit} (X ${dx}, Y ${dy}, Z ${dz})`,
  faceDistance: (d: string, unit: string) => `Face to face ${d} ${unit}`,
  angle: (degrees: string) => `Faces at ${degrees}°`,
  measuredExact: 'Snapped to exact faces',
  measuredGrid: 'Snapped to the simulation grid',
  exportTitle: 'Export Assembly',
  format: 'Format',
  formatName: { stl: 'STL', '3mf': '3MF' },
  exportAssembly: 'Export Assembly...',
  exported: (pieces: number, path: string) =>
    `${pieces === 1 ? '1 piece' : `${pieces} pieces`} written to ${path}.`,
} as const;
