import { describe, expect, it } from 'vitest';

import { gatherWarnings, toolpathName, warningCount } from './toolpathWarnings';

describe('gatherWarnings', () => {
  it('leaves out calculations with nothing to say', () => {
    expect(gatherWarnings([])).toEqual([]);
    expect(gatherWarnings([{ toolpath: 'Pocket 1', warnings: [] }])).toEqual([]);
    expect(gatherWarnings([{ toolpath: 'Pocket 1', warnings: ['  ', ''] }])).toEqual([]);
  });

  it('keeps one entry per toolpath in calculation order', () => {
    const warnings = gatherWarnings([
      { toolpath: 'Pocket 1', warnings: ['1 vector crossed itself and was repaired.'] },
      { toolpath: 'Profile 1', warnings: [] },
      { toolpath: 'VGroove 1', warnings: ['Flat depth limited.'] },
      { toolpath: 'Pocket 1', warnings: ['Cuts into a keep-out zone.'] },
    ]);
    expect(warnings).toEqual([
      {
        toolpath: 'Pocket 1',
        messages: ['1 vector crossed itself and was repaired.', 'Cuts into a keep-out zone.'],
      },
      { toolpath: 'VGroove 1', messages: ['Flat depth limited.'] },
    ]);
    expect(warningCount(warnings)).toBe(3);
  });

  it('drops repeats of a message for the same toolpath and trims them', () => {
    const warnings = gatherWarnings([
      { toolpath: 'Pocket 1', warnings: ['Timed out.', ' Timed out. '] },
      { toolpath: 'Pocket 2', warnings: ['Timed out.'] },
    ]);
    expect(warnings).toEqual([
      { toolpath: 'Pocket 1', messages: ['Timed out.'] },
      { toolpath: 'Pocket 2', messages: ['Timed out.'] },
    ]);
  });

  it('keeps warnings about no one toolpath under null', () => {
    expect(gatherWarnings([{ toolpath: null, warnings: ['A toolpath cuts below.'] }])).toEqual([
      { toolpath: null, messages: ['A toolpath cuts below.'] },
    ]);
  });
});

describe('toolpathName', () => {
  it('finds the name by id', () => {
    const list = [
      { id: 3, name: 'Pocket 1' },
      { id: 7, name: 'Profile 1' },
    ];
    expect(toolpathName(list, 7)).toBe('Profile 1');
    expect(toolpathName(list, 9)).toBeNull();
  });
});
