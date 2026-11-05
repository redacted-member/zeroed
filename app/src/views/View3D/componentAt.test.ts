import { describe, expect, it } from 'vitest';

import type { ComponentSummary } from '../../bridge/model';
import { componentAt } from './componentAt';

const part = (
  id: number,
  bbox: [number, number, number, number],
  visible = true,
): ComponentSummary => ({
  id,
  name: `Part ${id}`,
  visible,
  combine: 'add',
  heightScale: 1,
  level: 1,
  maxHeight: 1,
  bbox,
});

describe('picking a component', () => {
  it('prefers the topmost visible component under the point', () => {
    const list = [part(1, [0, 0, 100, 100]), part(2, [40, 40, 60, 60])];
    expect(componentAt(list, 50, 50)?.id).toBe(2);
    expect(componentAt(list, 10, 10)?.id).toBe(1);
    expect(componentAt(list, 150, 10)).toBeNull();
  });

  it('skips hidden components', () => {
    const list = [part(1, [0, 0, 100, 100]), part(2, [40, 40, 60, 60], false)];
    expect(componentAt(list, 50, 50)?.id).toBe(1);
  });
});
