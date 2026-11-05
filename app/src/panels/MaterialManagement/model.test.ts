import { describe, expect, it } from 'vitest';

import { machineSetupStrings } from '../../lib/strings/machineSetup';
import { nameProblem, nextAfterDelete, uniqueName } from './model';

describe('material management rules', () => {
  it('finds a name the list does not hold yet', () => {
    expect(uniqueName([], 'New Material')).toBe('New Material');
    expect(uniqueName(['New Material', 'new material 2'], 'New Material')).toBe('New Material 3');
  });

  it('refuses an empty name and one another material has', () => {
    const list = ['Oak', 'Pine'];
    expect(nameProblem(list, 'Oak', ' ')).toBe(machineSetupStrings.materials.emptyName);
    expect(nameProblem(list, 'Oak', 'Pine')).toBe(machineSetupStrings.materials.duplicateName);
    expect(nameProblem(list, 'Oak', 'Oak')).toBeNull();
    expect(nameProblem(list, 'Oak', 'Red Oak')).toBeNull();
  });

  it('activates the next material after a delete', () => {
    expect(nextAfterDelete(['Ash', 'Oak', 'Pine'], 'Oak')).toBe('Pine');
    expect(nextAfterDelete(['Ash', 'Oak', 'Pine'], 'Pine')).toBe('Oak');
    expect(nextAfterDelete(['Oak'], 'Oak')).toBe('');
  });
});
