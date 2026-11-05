import { describe, expect, it } from 'vitest';

import { jobMaterial, materialLook } from './appearance';
import { finishSlot } from './componentFinish';

const defaults = { material: 'theme' as const, solidColour: '#c9b28f' };

describe('job material', () => {
  it('draws the job in its own material whatever the preference default is', () => {
    const chosen = jobMaterial({ material: 'walnut', colour: '#123456' }, defaults);
    expect(chosen).toEqual({ material: 'walnut', colour: '#123456' });
    expect(materialLook(chosen.material, '#aaaaaa', chosen.colour).colour).toBe('#6a4631');
    expect(finishSlot(chosen.material)).toBeGreaterThan(0);
  });

  it('falls back to the preference for a job without an appearance', () => {
    expect(jobMaterial(undefined, defaults)).toEqual({ material: 'theme', colour: '#c9b28f' });
  });

  it('gives each material its own look and a solid color no grain', () => {
    const looks = (['pine', 'oak', 'walnut', 'maple', 'mdf'] as const).map(
      (m) => materialLook(m, '', '').colour,
    );
    expect(new Set(looks).size).toBe(looks.length);
    expect(materialLook('solid', '', '#2a8f3a').colour).toBe('#2a8f3a');
    expect(finishSlot('solid')).toBe(0);
  });
});
