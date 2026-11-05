// Decisions of the 3D view's window key listeners. Each one leaves keys
// alone while a modal dialog is open, so a key meant for the dialog (or
// typed with the focus left on the body after a click in it) cannot act on
// the view behind it.

import { modalDialogOpen } from '../../lib/dialogKeys';

/** The parts of a keyboard event the view's listeners read. */
export interface ViewKey {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  target: EventTarget | null;
}

/** Holding this key with the pointer over the view turns it slowly. */
export const TWIDDLE_KEY = 'w';

/** True while focus is in a field that takes typed text. */
export function typingIn(target: EventTarget | null): boolean {
  if (target === null || typeof target !== 'object') {
    return false;
  }
  const element = target as { tagName?: unknown; isContentEditable?: unknown };
  const tag = element.tagName;
  return (
    tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || element.isContentEditable === true
  );
}

/**
 * The shadow shading step Ctrl + NumPad plus or minus asks for: 1 deeper,
 * -1 lighter, 0 when the key is not one of them or does not apply.
 */
export function shadowKeyStep(event: ViewKey, shadowShading: boolean): -1 | 0 | 1 {
  const by = event.code === 'NumpadAdd' ? 1 : event.code === 'NumpadSubtract' ? -1 : 0;
  if (by === 0 || !event.ctrlKey || !shadowShading || modalDialogOpen()) {
    return 0;
  }
  return by;
}

/** Whether a key down starts the twiddle (the pointer over the view). */
export function startsTwiddle(event: ViewKey, hovering: boolean): boolean {
  return (
    event.key.toLowerCase() === TWIDDLE_KEY &&
    hovering &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey &&
    !typingIn(event.target) &&
    !modalDialogOpen()
  );
}

/** Whether a key down puts the 3D transform handles away (Escape, no drag). */
export function endsTransform(event: ViewKey, dragging: boolean): boolean {
  return event.key === 'Escape' && !dragging && !typingIn(event.target) && !modalDialogOpen();
}
