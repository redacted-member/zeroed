import { describe, expect, it } from 'vitest';

import { windowTitle } from './windowTitle';

describe('window title', () => {
  it('names the saved job file', () => {
    expect(windowTitle('/Users/a/jobs/sign.carve')).toBe('sign.carve - Zeroed');
  });

  it('says Untitled before the first save', () => {
    expect(windowTitle(null)).toBe('Untitled - Zeroed');
  });
});
