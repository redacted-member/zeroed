import { describe, expect, it } from 'vitest';

import { importMessage } from './importToolpaths';

describe('import toolpaths message', () => {
  it('counts the toolpaths and lists what was skipped', () => {
    expect(importMessage(1, [])).toBe('Imported 1 toolpath.');
    expect(importMessage(2, ['skipped G18', 'skipped A'])).toBe(
      'Imported 2 toolpaths. Some of the file was skipped: skipped G18, skipped A.',
    );
  });
});
