// The Shift and Ctrl (or Command) keys as last seen by the web view, for
// menu commands whose result depends on a key held while the item is
// chosen. Menu clicks carry no key state of their own, so every key and
// pointer event records it here.

export interface HeldModifiers {
  shift: boolean;
  /** Ctrl, or Command on a Mac. */
  ctrl: boolean;
}

interface ModifierEvent {
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}

let held: HeldModifiers = { shift: false, ctrl: false };

/** Records the keys an event says are down. */
export function noteModifiers(event: ModifierEvent): void {
  held = { shift: event.shiftKey, ctrl: event.ctrlKey || event.metaKey };
}

/** The keys down at the last key or pointer event. */
export function heldModifiers(): HeldModifiers {
  return held;
}

/** Forgets every key, as when the window loses focus. */
export function releaseModifiers(): void {
  held = { shift: false, ctrl: false };
}

const EVENTS = ['keydown', 'keyup', 'pointerdown', 'pointerup', 'pointermove'] as const;

/** Keeps the record up to date from `target`'s events; answers the stop. */
export function watchHeldModifiers(target: Window): () => void {
  const note = (event: Event) => {
    if (event instanceof KeyboardEvent || event instanceof MouseEvent) {
      noteModifiers(event);
    }
  };
  for (const name of EVENTS) {
    target.addEventListener(name, note, { capture: true, passive: true });
  }
  target.addEventListener('blur', releaseModifiers);
  return () => {
    for (const name of EVENTS) {
      target.removeEventListener(name, note, { capture: true });
    }
    target.removeEventListener('blur', releaseModifiers);
  };
}
