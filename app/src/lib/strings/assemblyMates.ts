// The Assembly View's piece tree (show, hide, isolate, ground) and its
// mates (the kinds, the list and editing one). Read only by the lazily
// loaded Assembly View form, so not spread into the start strings.

import type { MateKind, MateStanding } from '../../bridge/assembly';

export const assemblyMatesStrings = {
  tree: {
    show: (name: string) => `Show ${name}`,
    hide: (name: string) => `Hide ${name}`,
    isolate: (name: string) => `Show only ${name}`,
    endIsolate: 'Show the other pieces again',
    ground: (name: string) => `Ground ${name}`,
    unground: (name: string) => `Unground ${name}`,
    showAll: 'Show All',
    grounded: 'Grounded',
    hidden: 'Hidden',
    groundedHint: 'Grounded: it stays where it is. Unground it to move it.',
  },
  mates: {
    title: 'Mates',
    empty: 'No mates yet. Mate Pieces below ties a face of one piece to a face of another.',
    type: 'Type',
    kinds: {
      against: 'Against',
      flush: 'Flush',
      distance: 'Distance',
      angle: 'Angle',
      concentric: 'Concentric',
      joint: 'Joint',
    } satisfies Record<MateKind, string>,
    kindHints: {
      against: 'Lay a face of one piece against a face of another.',
      flush: 'Line a face up with a face of another piece, both looking the same way.',
      distance: 'Hold a face a set distance from a face of another piece.',
      angle: 'Hold a face at a set angle to a face of another piece.',
      concentric: 'Line up the axes of two round faces, such as a dowel and its hole.',
      joint:
        'Holds two parts as their Joint Creator joint puts them. Remove the joint in the Joint Creator to take it off.',
    } satisfies Record<MateKind, string>,
    /** A mate in the list: its kind and its two pieces. */
    row: (kind: string, a: string, b: string) => `${kind}: ${a}, ${b}`,
    states: {
      solved: 'Solved',
      conflict: 'Conflict',
      suppressed: 'Suppressed',
      missing: 'Piece missing',
    } satisfies Record<MateStanding, string>,
    /** How far a mate in conflict is off. */
    off: (distance: string, angle: string) => `Off by ${distance} and ${angle}°`,
    offset: 'Offset',
    distance: 'Distance',
    angle: 'Angle',
    aligned: 'Faces look the same way',
    edit: (name: string) => `Edit Mate: ${name}`,
    apply: 'Apply',
    suppress: 'Suppress',
    unsuppress: 'Unsuppress',
    delete: 'Delete',
    pickRoundTarget: 'Click the round side of the hole or pin that stays in place.',
    pickRoundMoving: 'Now click the round side of the hole or pin on the piece that moves.',
    conflicts: (count: number) =>
      count === 1 ? '1 mate is in conflict.' : `${count} mates are in conflict.`,
  },
} as const;
