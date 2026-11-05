import { describe, expect, it } from 'vitest';

import { baseNameOf, INLAY_GROUPS, kindOf, kindParams, type InlayKind } from './model';

describe('create inlay toolpath chooser', () => {
  it('offers the male then the female kinds', () => {
    expect(INLAY_GROUPS.map((g) => g.kinds)).toEqual([
      ['maleStraight', 'maleStepped'],
      ['femalePocket', 'femaleHole'],
    ]);
  });

  it('round trips each kind through the kernel settings', () => {
    const kinds: InlayKind[] = ['maleStraight', 'maleStepped', 'femalePocket', 'femaleHole'];
    for (const kind of kinds) {
      expect(kindOf(kindParams(kind))).toBe(kind);
    }
    expect(kindParams('femaleHole')).toMatchObject({ part: 'female', female: 'hole' });
    expect(baseNameOf('maleStepped')).toBe('Male Inlay');
    expect(baseNameOf('femalePocket')).toBe('Female Inlay');
  });
});
