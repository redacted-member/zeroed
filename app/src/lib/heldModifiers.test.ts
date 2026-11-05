import { afterEach, describe, expect, it } from 'vitest';

import { heldModifiers, noteModifiers, releaseModifiers } from './heldModifiers';

describe('held modifiers', () => {
  afterEach(releaseModifiers);

  it('starts with nothing held and follows the last event', () => {
    expect(heldModifiers()).toEqual({ shift: false, ctrl: false });
    noteModifiers({ shiftKey: true, ctrlKey: false, metaKey: false });
    expect(heldModifiers()).toEqual({ shift: true, ctrl: false });
    noteModifiers({ shiftKey: false, ctrlKey: false, metaKey: true });
    expect(heldModifiers()).toEqual({ shift: false, ctrl: true });
    releaseModifiers();
    expect(heldModifiers()).toEqual({ shift: false, ctrl: false });
  });
});
