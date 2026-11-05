import { describe, expect, it } from 'vitest';

import { nodeToggle } from './quickNodeEdit';

describe('nodeToggle', () => {
  it('leaves a drawing form for node editing', () => {
    expect(nodeToggle('create', true)).toEqual({ tool: 'node', closeForm: true });
  });

  it('toggles between select and node otherwise', () => {
    expect(nodeToggle('select', true)).toEqual({ tool: 'node', closeForm: false });
    expect(nodeToggle('select', false)).toEqual({ tool: 'node', closeForm: false });
    expect(nodeToggle('node', false)).toEqual({ tool: 'select', closeForm: false });
  });
});
