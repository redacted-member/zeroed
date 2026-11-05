// Graphics tablet support for sculpting: a pen's pressure scales the brush
// strength when Preferences, General, Use a graphics tablet is on.

import { usePrefsStore } from '../../state/prefsStore';

/** The pen pressure (0 to 1) of a pointer event to sculpt with, or null
 * when the pointer is not a pen or tablet support is off. */
export function penPressure(
  event: { pointerType: string; pressure: number },
  tablet: boolean = usePrefsStore.getState().prefs.general.graphicsTablet,
): number | null {
  if (!tablet || event.pointerType !== 'pen') {
    return null;
  }
  return Number.isFinite(event.pressure) ? Math.min(Math.max(event.pressure, 0), 1) : null;
}
