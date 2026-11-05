// Pictures of the joints for the Joint Creator gallery, as shell/ToolIcons
// specs (24 by 24, drawn with drawIcon): each shows the two parts seen from
// above, the first outlined and the second tinted.

import type { JointKind } from '../../bridge/joints';

export const JOINT_ICONS: Readonly<Partial<Record<JointKind, string>>> = {
  // Two boards whose fingers interlock along the seam.
  finger: 'M2 5h8v3h3v3h-3v3h3v3h-3v2H2Z;n:M22 5H10v3h3v3h-3v3h3v3h-3v2h12Z',
  // Seen from the side: a board standing in a groove across the other.
  dado: 'M2 14h8v3h4v-3h8v7H2Z;n:M10 3h4v14h-4Z',
  // A board sitting in a step along the other's end.
  rabbet: 'M2 15h14v3h6v3H2Z;n:M16 3h6v15h-6Z',
  // A board whose tabs go down through slots in the other.
  tabSlot: 'M2 15h20v5H2Z;n:M4 4h16v11h-3v5h-3v-5h-4v5H7v-5H4Z',
  // A tenon through the other part, held by a wedge through the tenon.
  wedgedTenon: 'M2 7h20v5H2Z;n:M9 2h6v19H9Z;M6 15h12l-1.5 3.5h-9Z',
  // A tenon through the other part, split by a kerf with a wedge in it.
  kerfTenon: 'M2 7h20v5H2Z;n:M8 2h8v10h-3V6h-2v6H8Z;M11 2h2l-.5 5h-1Z',
  // Tails on one panel's edge locked into sockets in the other.
  dovetail:
    'M2 4h9v3l4-1.5v5L11 9v5l4-1.5v5L11 16v4H2Z;n:M22 4H11v3l4-1.5v5L11 9v5l4-1.5v5L11 16v4h11Z',
  // Seen from the outside of a corner: tails through the other board.
  cornerDovetail: 'M2 3h9v18H2Z;n:M11 3h11v18H11Z;M11 5l4-1v6l-4-1ZM11 14l4-1v6l-4-1Z',
  // Dowels across the joint of a board standing on another.
  dowel: 'M2 14h20v6H2Z;n:M5 3h14v11H5Z;t:M8 9h2v8H8ZM14 9h2v8h-2Z',
  // A bolt through one part into a square nut in the other.
  captiveNut: 'M2 15h20v5H2Z;n:M5 3h14v12H5Z;M11 7h2v13h-2Z;r8 8 8 3',
  // A bow tie key across the seam of two panels.
  butterfly: 'r3 3 18 18;t:M12 3v18;n:M5 7l7 3 7-3v10l-7-3-7 3Z',
  // A round knob on a neck locked into a socket.
  puzzle: 'M3 4h8v5.5h1a3 3 0 1 1 0 5h-1V20H3Z;n:M21 4H11v5.5h1a3 3 0 1 1 0 5h-1V20h10Z',
  // One panel with staggered slots across it, so it bends.
  livingHinge: 'r3 5 18 14;t:M8 7v4M8 13v4M11 9v6M14 7v4M14 13v4M17 9v6',
  // Two parts crossing, each slotted half way (seen from the end).
  crossLap: 'M3 9h7V4h4v5h7v6h-7v5h-4v-5H3Z;n:M10 4h4v16h-4Z',
  // Two ends overlapped at a corner, each cleared half way.
  halfLap: 'M3 4h7v17H3Z;n:M3 14h18v7H3Z;t:M10 14v7',
  // Seen end on: a flat panel and one leaning on it, fingers meeting in a V.
  vJoint: 'M2 15h12v4H2Z;n:M14 15l5-10.5 3.4 1.6-5 10.5Z;t:M14 19l3-2.4',
  // A part through a panel (end on) with a quarter turn arrow.
  twistLock: 'M2 13h20v4H2Z;n:M10 3h4v19h-4Z;t:M5 8a7 3 0 0 1 14 0M19 8l-2.5-.5M19 8l.5-2.5',
  // A cam disc in the lower board pulling a bolt from the upper one.
  camLock: 'M2 14h20v6H2Z;n:M7 3h10v11H7Z;c12 17 2.5;t:M12 6v11',
  // A thin panel in a groove near the edge of a thicker one.
  groove: 'M2 14h15v3h2v-3h3v7H2Z;n:M17 3h2v14h-2Z',
  // A board standing on another, held by two countersunk screws from below.
  screws: 'M2 14h20v6H2Z;n:M6 3h12v11H6Z;t:M9 20V8M15 20V8;M7.5 20h3l-1.5-2ZM13.5 20h3l-1.5-2Z',
  // One wide tenon in a mortise.
  mortiseTenon: 'M2 14h20v6H2Z;n:M6 3h12v11h-3v4H9v-4H6Z',
};
