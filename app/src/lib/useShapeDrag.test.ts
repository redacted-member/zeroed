import { describe, expect, it } from 'vitest';

import type { DrawPointer } from '../state/drawStore';
import { ShapeDrag } from './useShapeDrag';

const at = (x: number, y: number): DrawPointer => ({
  x,
  y,
  rawX: x,
  rawY: y,
  shift: false,
  ctrl: false,
});

describe('one drag shape', () => {
  it('makes the shape once when Enter is pressed during a drag', () => {
    const drag = new ShapeDrag();
    drag.down(at(0, 0));
    expect(drag.move(at(40, 20))?.kind).toBe('drag');
    expect(drag.enter()).toEqual({
      kind: 'commit',
      from: { x: 0, y: 0 },
      to: { x: 40, y: 20 },
      pointer: at(40, 20),
    });
    expect(drag.up(at(40, 20))).toBeNull();
  });

  it('creates from the fields on Enter with no drag, and a drag commits on release', () => {
    const drag = new ShapeDrag();
    expect(drag.enter()).toEqual({ kind: 'enter' });
    drag.down(at(0, 0));
    drag.move(at(10, 10));
    expect(drag.up(at(10, 10))?.kind).toBe('commit');
    drag.down(at(5, 5));
    expect(drag.up(at(5, 5))?.kind).toBe('click');
  });
});
