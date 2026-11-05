// The Assembly View: its form in the Toolpaths panel, the piece handles in
// the 3D view, the clearance check and the cut list. Spread in
// lib/formStrings; the start bundle reads the title from strings/startup.

import { startupStrings } from './startup';

export const assemblyStrings = {
  assembly: {
    title: startupStrings.formTitles.assemblyView.title,
    toolHint: 'Put the machined parts together in the 3D view',
    pieces: 'Pieces',
    /** A sheet's only piece is named after the sheet, else sheet and part number. */
    sheetPiece: (sheet: string) => sheet,
    partPiece: (sheet: string, part: number) => `${sheet} part ${part}`,
    /** A sheet's raw model as a piece. */
    modelPiece: (sheet: string) => `${sheet} model`,
    /** A component piece whose component has no name. */
    componentPiece: (id: number) => `Component ${id}`,
    addComponent: 'Add Component',
    component: 'Component',
    addComponentHint:
      "Bring a component from the Component Tree in as a piece of its own, such as a mockup of a bought part to check the fit. It leaves its sheet's model piece.",
    /** A component in the add list, with its sheet when the job has several. */
    componentOption: (name: string, sheet: string | null) =>
      sheet === null ? name : `${name} (${sheet})`,
    noComponents: 'Every component is already a piece.',
    jobHasNoComponents: 'The job has no components yet.',
    add: 'Add',
    /** The two ways in, said in one line with a drag icon. */
    addComponentWays: 'Pick one below, or drag one from the Components tab onto the 3D view.',
    searchComponents: 'Search components',
    noComponentMatch: (query: string) => `No component matches "${query}".`,
    addBeside: (name: string) => `Add ${name} beside the pieces`,
    addBesideDescription: 'Stands it on the ground next to the other pieces.',
    place: 'Place',
    placeTitle: (name: string) => `Place ${name} with a click in the 3D view`,
    placeDescription: 'Its ghost follows the pointer; click to set it down, Esc cancels.',
    placing: (name: string) =>
      `Click in the 3D view to set ${name} down, on a piece or the ground. Esc cancels.`,
    cancelPlace: 'Cancel placing',
    /** The Components tab's line while the Assembly View is open. */
    treeDragHint:
      'Drag a component within the tree to reorder it, or onto the 3D view to add it to the Assembly View; one that is a piece already moves there.',
    treeDragRow: (name: string) => `Drag ${name} to reorder it, or onto the 3D view to place it`,
    removePiece: 'Remove Piece',
    empty: 'No pieces yet. Calculate toolpaths on the sheets first.',
    loading: 'Simulating the sheets...',
    guide: {
      title: 'How to build your own assembly',
      steps: [
        {
          name: 'Draw the parts.',
          text: 'Draw each part as a closed shape, or start from New Furniture below.',
        },
        {
          name: 'Join them.',
          text: 'Where two parts meet, open the Joint Creator (Joinery on the Design tab) to cut matching joints.',
        },
        {
          name: 'Cut them free.',
          text: 'Make a toolpath that cuts each part out, such as Profile. Every part it cuts free becomes a piece in the list. Mate Pieces then lays one face against another.',
        },
        {
          name: 'Check it.',
          text: 'Run Verify Fit, then use Explode under Pieces to pull the assembly apart and see how it goes together.',
        },
      ],
    },
    hint: 'Each part the toolpaths cut free is a piece; a sheet with a model and no toolpaths gives its model. Click a piece in the 3D view to pick it, then drag its arrows and rings, or type its place below.',
    snap: 'Snap to nearby pieces',
    placement: 'Placement',
    source: 'Piece from',
    sourceSimulation: 'Machined simulation',
    sourceModel: 'Raw model',
    sourceHint: "Use the sheet's model as it is drawn, or the parts its toolpaths cut.",
    position: 'Position',
    rotation: 'Rotation',
    axisLabel: (what: string, axis: string) => `${what} ${axis}`,
    apply: 'Apply',
    noneSelected: 'Pick a piece to place it by value.',
    badValue: 'Enter a number.',
    handles: 'Piece handles',
    arrow: (axis: string) => `Drag to move along ${axis}, click to type a distance`,
    ring: (axis: string) => `Drag to turn about ${axis}, click to type an angle`,
    ball: 'Drag to move the piece across the view',
    moveBy: (axis: string) => `Move along ${axis}`,
    turnBy: (axis: string) => `Turn about ${axis}`,
    canvasHint: 'Assembly View: click a piece to pick it',
    mate: 'Mate Pieces',
    mateHint: 'Lay a face of one piece against a face of another.',
    pickFaces: 'Pick Faces',
    cancel: 'Cancel',
    pickTarget: 'Click the face that stays in place.',
    pickMoving: 'Now click the face on the piece that moves to meet it.',
    mated: (moving: string, target: string) => `${moving} meets ${target}.`,
    offset: 'Offset',
    flip: 'Flip (turn the moving piece over)',
    turn: 'Turn',
    clearance: 'Clearance Check',
    minimum: 'Minimum clearance',
    check: 'Check',
    checking: 'Checking...',
    noIssues: 'No pieces overlap or come closer than the clearance.',
    issuesFound: (count: number) =>
      count === 1 ? '1 pair of pieces needs a look.' : `${count} pairs of pieces need a look.`,
    overlap: (a: string, b: string, volume: string, unit: string) =>
      `${a} and ${b} overlap by ${volume} ${unit}`,
    tooClose: (a: string, b: string, clearance: string, unit: string) =>
      `${a} and ${b} are closer than ${clearance} ${unit}`,
    showIssue: (label: string) => `Pick the pieces of: ${label}`,
    cutList: 'Cut List',
    cutListEmpty: 'No pieces to list.',
    refresh: 'Refresh',
    exportCsv: 'Export CSV',
    exportTitle: 'Export Cut List',
    csvFiles: 'CSV files',
    exported: (path: string) => `Cut list written to ${path}.`,
    downloaded: (name: string) => `Cut list saved as the download ${name}.`,
    columns: {
      piece: 'Piece',
      count: 'Qty',
      length: 'Length',
      width: 'Width',
      thickness: 'Thick',
    },
    csvHeader: ['Piece', 'Count', 'Length', 'Width', 'Thickness'],
  },
} as const;
