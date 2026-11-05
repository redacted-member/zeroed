// Collects Quick Keys typed into the drawing view for the active tool. The
// typed text lives in the draw store so the status bar can show it.

import { parseQuickEntry, quickKeyAction, type QuickEntry } from '../../../lib/quickKeys';
import { useDrawStore } from '../../../state/drawStore';

/**
 * Handles one key. `enabled` says whether the tool takes Quick Keys right
 * now; `apply` receives the parsed entry on Enter. Returns true when the key
 * was consumed.
 */
export function quickKeyDown(
  event: KeyboardEvent,
  enabled: boolean,
  apply: (entry: QuickEntry) => void,
): boolean {
  const draw = useDrawStore.getState();
  const text = draw.quickText;
  if (!enabled) {
    if (text !== '') {
      draw.setQuickText('');
    }
    return false;
  }
  switch (quickKeyAction(text, event.key)) {
    case 'ignore':
      return false;
    case 'append':
      draw.setQuickText(text + event.key.toLowerCase());
      return true;
    case 'erase':
      draw.setQuickText(text.slice(0, -1));
      return true;
    case 'clear':
      draw.setQuickText('');
      return true;
    case 'commit': {
      draw.setQuickText('');
      const entry = parseQuickEntry(text);
      if (entry !== null) {
        apply(entry);
      }
      return true;
    }
  }
}

/** Drops anything typed so far. */
export function clearQuickKeys(): void {
  const draw = useDrawStore.getState();
  if (draw.quickText !== '') {
    draw.setQuickText('');
  }
}
