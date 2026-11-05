// The groups toolpath moves are drawn in, each in its own colour, and which
// of them Toolpaths > Toolpath Drawing shows. Cuts always show; rapids,
// plunges, retracts and joins each have a menu toggle.

import { MOVE_KIND } from '../bridge/cam';
import type { ToolpathPrefs } from '../bridge/prefs';

export type MoveGroup = 'cut' | 'rapid' | 'plunge' | 'retract' | 'join';

export const MOVE_GROUPS: readonly MoveGroup[] = ['cut', 'rapid', 'plunge', 'retract', 'join'];

/** Which groups are drawn. */
export type MoveVisibility = Readonly<Record<MoveGroup, boolean>>;

export const ALL_MOVES: MoveVisibility = {
  cut: true,
  rapid: true,
  plunge: true,
  retract: true,
  join: true,
};

/** The group of a `cam.moves` kind, or null for a dwell, which has no length. */
export function moveGroup(kind: number | undefined): MoveGroup | null {
  switch (kind) {
    case MOVE_KIND.rapid:
      return 'rapid';
    case MOVE_KIND.plunge:
      return 'plunge';
    case MOVE_KIND.retract:
      return 'retract';
    case MOVE_KIND.join:
      return 'join';
    case MOVE_KIND.feed:
    case MOVE_KIND.arc:
      return 'cut';
    default:
      return null;
  }
}

/** The groups the drawing preferences show. */
export function moveVisibility(prefs: ToolpathPrefs): MoveVisibility {
  return {
    cut: true,
    rapid: prefs.drawRapids,
    plunge: prefs.drawPlunges,
    retract: prefs.drawRetracts,
    join: prefs.drawJoins,
  };
}
