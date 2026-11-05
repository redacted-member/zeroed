// Tooltip text and placement. A control opts in with a data-tip attribute
// holding its tooltip text; the one Tooltip layer (Tooltip.tsx) shows it on
// hover and on keyboard focus.

import { IS_MAC } from '../../lib/keyLabels';
import type { AppCommand } from '../../lib/shortcuts';
import { shortcutLabel } from '../MenuBar/model';

/** Waits this long on a control before its tooltip shows. */
export const TIP_DELAY_MS = 450;
/** Space between the control and its tooltip, and kept from the window edges. */
const GAP = 6;

/**
 * A tooltip's text: the name, its shortcut in brackets when it has one,
 * and a one line description on a second line when given.
 */
export function tipText(name: string, shortcut?: string, description?: string): string {
  const head = shortcut === undefined ? name : `${name} (${shortcut})`;
  return description === undefined ? head : `${head}\n${description}`;
}

/**
 * The tooltip of a button that may be off. While `why` says what it waits
 * for, that takes the place of the description line, so the tooltip tells
 * why the button cannot be used; otherwise the tooltip is `tip` as it is.
 * The button is off exactly while `why` is given.
 */
export function whyTip(tip: string, why: string | undefined): string {
  if (why === undefined) {
    return tip;
  }
  const name = tip.split('\n')[0] ?? tip;
  return `${name}\n${why}`;
}

/**
 * The reason of the first of `checks` that holds, or undefined when none
 * does: each check is a condition that turns a button off and the plain
 * words saying so.
 */
export function firstWhy(...checks: ReadonlyArray<readonly [boolean, string]>): string | undefined {
  return checks.find(([off]) => off)?.[1];
}

/**
 * The tooltip of a control that runs `command`: its name plus the
 * command's key, and `description` on a second line when given.
 */
export function commandTip(
  name: string,
  command: AppCommand | undefined,
  description?: string,
): string {
  return tipText(
    name,
    command === undefined ? undefined : shortcutLabel(command, IS_MAC),
    description,
  );
}

/**
 * Whether a tooltip may open now. A click (or Esc) ends the tooltips until
 * the pointer moves again, so what the click puts under a still pointer
 * (a form replacing a panel) does not pop one up at once; keyboard focus
 * still shows one.
 */
export function tipAllowed(clickedSinceMove: boolean, viaKeyboard: boolean): boolean {
  return viaKeyboard || !clickedSinceMove;
}

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Where a tooltip of `size` goes for a control at `target` in a window of
 * `view`: centered below the control, above it when there is no room
 * below, and moved in so it never runs off the window.
 */
export function placeTip(
  target: Box,
  size: { width: number; height: number },
  view: { width: number; height: number },
): { left: number; top: number } {
  const clamp = (value: number, max: number) => Math.max(GAP, Math.min(value, max - GAP));
  const below = target.top + target.height + GAP;
  const top = below + size.height <= view.height - GAP ? below : target.top - GAP - size.height;
  return {
    left: clamp(target.left + (target.width - size.width) / 2, view.width - size.width),
    top: clamp(top, view.height - size.height),
  };
}

/**
 * The disabled controls that carry a tooltip. A disabled control gets no
 * pointer events (WebKit sends it none; the style sheet lets the pointer
 * through to what holds it), so the tooltip layer finds it under the
 * pointer instead.
 */
export const DISABLED_TIP = 'button:disabled[data-tip]';

/**
 * Which of `boxes` (on screen, in document order) the point lies in: the
 * last one when they overlap, as it is drawn on top; -1 for none.
 */
export function boxAt(boxes: readonly Box[], x: number, y: number): number {
  for (let i = boxes.length - 1; i >= 0; i -= 1) {
    const box = boxes[i];
    if (
      box !== undefined &&
      x >= box.left &&
      x < box.left + box.width &&
      y >= box.top &&
      y < box.top + box.height
    ) {
      return i;
    }
  }
  return -1;
}
