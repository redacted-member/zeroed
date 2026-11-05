import { describe, expect, it } from 'vitest';

import { panelLabel } from './usePanelUnder';

describe('panelLabel', () => {
  it('names the panel and the face seen from above in furniture words', () => {
    expect(
      panelLabel({ design: 1, panel: 'door#0', name: 'Door 1', top: 'front', bottom: 'back' }),
    ).toBe('Door 1, front face');
  });

  it('says nothing off a panel', () => {
    expect(panelLabel({ design: null, panel: null, name: null, top: null, bottom: null })).toBe(
      null,
    );
  });
});
