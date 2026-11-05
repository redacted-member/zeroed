import { describe, expect, it } from 'vitest';

import { nextToolpathName, TOOLPATH_BASE_NAMES } from './toolpathNames';

describe('nextToolpathName', () => {
  it('starts at one', () => {
    expect(nextToolpathName('Profile', [])).toBe('Profile 1');
  });

  it('follows the highest number of the same base', () => {
    expect(nextToolpathName('Profile', ['Profile 1', 'Profile 3', 'Pocket 7'])).toBe('Profile 4');
  });

  it('reads numbers followed by a tool name and ignores other text', () => {
    expect(
      nextToolpathName('Pocket', ['Pocket 2 [End Mill (6 mm)]', 'Pocket edge', 'Pockets 9']),
    ).toBe('Pocket 3');
  });

  it('uses the names', () => {
    expect(TOOLPATH_BASE_NAMES.finish3d).toBe('3D Finish');
    expect(TOOLPATH_BASE_NAMES.moulding).toBe('Swept Profile');
    expect(TOOLPATH_BASE_NAMES.threadMilling).toBe('Thread');
    expect(TOOLPATH_BASE_NAMES.inlay).toBe('VGroove Inlay');
  });
});
