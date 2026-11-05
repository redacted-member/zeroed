// The composite model volume line of the 3D view (Ctrl+Shift+Alt+V). Pure.

import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';

const MM3_PER_IN3 = 25.4 ** 3;

/** A volume in cubic millimetres as the job's units write it. */
export function volumeText(volumeMm3: number, units: Units): string {
  if (units === 'inch') {
    return `${(volumeMm3 / MM3_PER_IN3).toFixed(2)} in³`;
  }
  return `${Math.round(volumeMm3).toLocaleString('en-US')} mm³`;
}

/** The line the 3D view shows, or null when the measured volume is stale
 * (the model changed since) or hidden. */
export function volumeLine(
  shown: { volume: number; empty: boolean; selected: number; stamp: number | null } | null,
  stamp: number | null,
  units: Units,
): string | null {
  if (shown === null || shown.stamp !== stamp) {
    return null;
  }
  if (shown.empty) {
    return strings.views.noModelVolume;
  }
  const text = volumeText(shown.volume, units);
  return shown.selected > 0
    ? strings.views.selectedVolume(shown.selected, text)
    : strings.views.modelVolume(text);
}
