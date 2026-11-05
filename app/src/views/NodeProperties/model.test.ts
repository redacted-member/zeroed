import { describe, expect, it } from 'vitest';

import { moveCommand, nodeFields } from './model';

describe('node properties model', () => {
  it('shows the node in the display unit and sends millimetres', () => {
    const fields = nodeFields({ x: 25.4, y: 50.8 }, 'inch');
    expect(fields).toEqual({ x: '1', y: '2' });
    expect(moveCommand({ x: '2', y: '1' }, 'inch')).toBe('node-xy:50.8,25.4');
    expect(moveCommand({ x: 'a', y: '1' }, 'mm')).toBeNull();
  });
});
