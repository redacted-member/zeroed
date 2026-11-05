import { describe, expect, it } from 'vitest';

import {
  defaultTemplate,
  drillIndex,
  isReference,
  resized,
  templateProblem,
  withDrill,
  withReference,
} from './model';

describe('drill bank template', () => {
  it('starts as a row of nine drills on the 32 mm pitch', () => {
    const bank = defaultTemplate();
    expect(bank.drills).toHaveLength(9);
    expect(bank.pitch).toBe(32);
    expect(isReference(bank, 0)).toBe(true);
    expect(templateProblem(bank)).toBeNull();
  });

  it('keeps drills in place when resized and numbers the new ones', () => {
    const bank = withDrill(defaultTemplate(), 2, { diameter: 8 });
    const grid = resized(bank, 3, 2);
    expect(grid.drills).toHaveLength(6);
    expect(grid.drills[2]).toEqual({ number: 3, diameter: 8 });
    expect(grid.drills.slice(3).map((d) => d.number)).toEqual([4, 5, 6]);
    const smaller = resized(withReference(grid, drillIndex(grid, 2, 1)), 1, 1);
    expect(smaller.reference).toEqual([0, 0]);
    expect(resized(bank, 0, 50).vertical).toBe(20);
  });

  it('refuses a bank the kernel would refuse', () => {
    const bank = defaultTemplate();
    expect(templateProblem({ ...bank, pitch: 0 })).toBe('pitch');
    expect(
      templateProblem({ ...bank, drills: bank.drills.map((d) => ({ ...d, number: 0 })) }),
    ).toBe('noDrills');
    expect(templateProblem(withDrill(bank, 1, { diameter: 0 }))).toBe('diameter');
    expect(templateProblem(withDrill(bank, 1, { number: 1 }))).toBe('duplicate');
    // A position with no drill fitted needs no diameter.
    expect(templateProblem(withDrill(bank, 1, { number: 0, diameter: 0 }))).toBeNull();
  });
});
