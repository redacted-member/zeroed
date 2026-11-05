import { describe, expect, it } from 'vitest';

import { crossingIds, crossingOutcome, crossingQuestion, type Crossing } from './intersectionCheck';

const found: Crossing[] = [
  { ids: [1, 2], x: 5, y: 5 },
  { ids: [2], x: 9, y: 1 },
];

describe('intersection check', () => {
  it('calculates when nothing crosses or the check is off', async () => {
    expect(await crossingOutcome('ask', [], () => false)).toBe('go');
    expect(await crossingOutcome('skip', found, () => false)).toBe('go');
    expect(await crossingOutcome('proceed', found, () => false)).toBe('go');
  });

  it('opens the validator or asks as the preference says', async () => {
    expect(await crossingOutcome('validator', found, () => true)).toBe('validator');
    expect(await crossingOutcome('ask', found, () => true)).toBe('go');
    expect(await crossingOutcome('ask', found, (n) => n > 5)).toBe('validator');
  });

  it('lists each crossing vector once', () => {
    expect(crossingIds(found)).toEqual([1, 2]);
  });

  it('asks about the offset before an offset, the toolpath before a calculation', () => {
    expect(crossingQuestion(1, 'offset')).toMatch(/offset\. Offset anyway\?/);
    expect(crossingQuestion(1, 'offset')).not.toMatch(/toolpath/);
    expect(crossingQuestion(2, 'toolpath')).toMatch(/2 places, which can spoil the toolpath/);
  });
});
