// Which camera move a mouse drag makes in the 3D view, for the two mouse
// schemes the preferences offer.

import type { Navigation3d } from '../../bridge/prefs';

export type CameraDrag = 'orbit' | 'pan' | 'dolly';

const LEFT = 0;
const MIDDLE = 1;
const RIGHT = 2;

/** How fast a vertical drag zooms: the distance scales by e per this many pixels. */
const DOLLY_PIXELS = 200;

/**
 * The camera move for a drag started with `button` (the pointer event's
 * button number), or null when that button does not move the camera.
 * Both schemes rotate with the left button, pan with the middle one and
 * pan with Shift and the left button; they differ on the right button,
 * which pans in the modern scheme and zooms in the legacy one.
 */
export function dragFor(button: number, shift: boolean, scheme: Navigation3d): CameraDrag | null {
  if (button === LEFT) {
    return shift ? 'pan' : 'orbit';
  }
  if (button === MIDDLE) {
    return 'pan';
  }
  if (button === RIGHT) {
    return scheme === 'legacy' ? 'dolly' : 'pan';
  }
  return null;
}

/**
 * The camera move for a drag while sculpting, where the left button
 * sculpts: Alt with the left button or the right button turns the view
 * (twiddle), the middle button pans. Null means the drag sculpts, or does
 * nothing.
 */
export function sculptDragFor(button: number, alt: boolean): CameraDrag | null {
  if (button === LEFT) {
    // Shift with the left button sculpts too (the Undo Brush erases).
    return alt ? 'orbit' : null;
  }
  if (button === MIDDLE) {
    return 'pan';
  }
  if (button === RIGHT) {
    return 'orbit';
  }
  return null;
}

/** What the mouse wheel does while sculpting: Alt zooms the view, Shift
 * changes the brush strength, otherwise the diameter (finer with Ctrl or
 * Cmd). */
export function sculptWheel(keys: {
  alt: boolean;
  shift: boolean;
  ctrl: boolean;
}): 'zoom' | 'strength' | 'diameter' | 'fineDiameter' {
  if (keys.alt) {
    return 'zoom';
  }
  if (keys.shift) {
    return 'strength';
  }
  return keys.ctrl ? 'fineDiameter' : 'diameter';
}

/** Distance factor for a dolly drag of `deltaY` pixels (down moves away). */
export function dollyFactor(deltaY: number): number {
  return Math.exp(deltaY / DOLLY_PIXELS);
}
