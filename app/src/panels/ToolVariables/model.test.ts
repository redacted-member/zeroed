import { describe, expect, it } from 'vitest';

import { withVariable, withoutVariable } from './model';

describe('tool variables', () => {
  it('keeps the list sorted and one entry per name', () => {
    let list = withVariable([], 'Maker', 'Acme');
    list = withVariable(list, 'Coating', 'TiN');
    list = withVariable(list, ' Maker ', 'Other');
    expect(list).toEqual([
      { name: 'Coating', value: 'TiN' },
      { name: 'Maker', value: 'Other' },
    ]);
    expect(withoutVariable(list, 'Coating')).toEqual([{ name: 'Maker', value: 'Other' }]);
  });
});
