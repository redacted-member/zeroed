import { describe, expect, it } from 'vitest';

import { volumeLine, volumeText } from './modelVolume';

describe('model volume line', () => {
  it('writes the volume in the job units', () => {
    expect(volumeText(40_000, 'mm')).toBe('40,000 mm³');
    expect(volumeText(25.4 ** 3 * 2, 'inch')).toBe('2.00 in³');
  });

  it('shows only while the model is the one measured', () => {
    const shown = { volume: 1000, empty: false, selected: 0, stamp: 4 };
    expect(volumeLine(shown, 4, 'mm')).toContain('1,000 mm³');
    expect(volumeLine(shown, 5, 'mm')).toBeNull();
    expect(volumeLine(null, 4, 'mm')).toBeNull();
    expect(volumeLine({ ...shown, empty: true }, 4, 'mm')).not.toContain('mm³');
    const picked = volumeLine({ ...shown, selected: 2 }, 4, 'mm');
    expect(picked).toContain('2 selected components');
    expect(picked).toContain('1,000 mm³');
  });
});
