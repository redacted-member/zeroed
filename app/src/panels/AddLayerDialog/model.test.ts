import { describe, expect, it } from 'vitest';

import { newLayerDraft, newLayerRequest, nextLayerName } from './model';

describe('Add New Layer dialog', () => {
  it('offers the next free layer name and the next layer colour', () => {
    expect(nextLayerName([{ name: 'Layer 1' }])).toBe('Layer 2');
    expect(nextLayerName([{ name: 'Cut' }, { name: 'layer 3' }])).toBe('Layer 4');
    const draft = newLayerDraft([{ name: 'Layer 1' }, { name: 'Cut' }]);
    expect(draft).toEqual({ name: 'Layer 3', colour: '#1f6fd6', visible: true, active: true });
  });

  it('refuses a blank name and passes visibility, activity and position', () => {
    const draft = { name: '  ', colour: '#112233', visible: true, active: true };
    expect(newLayerRequest(draft)).toBeNull();
    expect(
      newLayerRequest({ ...draft, name: ' Engrave ', visible: false, active: false }, 7),
    ).toEqual({
      name: 'Engrave',
      colour: '#112233',
      options: { visible: false, active: false, before: 7 },
    });
    expect(newLayerRequest({ ...draft, name: 'Cut' })?.options).toEqual({
      visible: true,
      active: true,
    });
  });
});
