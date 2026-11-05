import { describe, expect, it } from 'vitest';

import type { PostSummary } from '../../bridge/cam';
import type { MachinePrefs } from '../../bridge/prefs';
import { machineWrapsBlank } from './model';

const post = (id: string, y: string | null, x: string | null) =>
  ({ id, name: id, rotaryWrapY: y, rotaryWrapX: x }) as PostSummary;
const posts = [post('flat', null, null), post('along-x', 'A', null), post('both', 'A', 'A')];
const machines = (list: string[]): MachinePrefs =>
  ({
    active: 'Router',
    list: [{ name: 'Router', units: 'inch', bedX: 1, bedY: 1, bedZ: 1, posts: list }],
  }) as MachinePrefs;

describe('machineWrapsBlank', () => {
  it('counts every post while the machine has none of its own', () => {
    expect(machineWrapsBlank(posts, machines([]), 'x')).toBe(true);
    expect(machineWrapsBlank(posts, machines([]), 'y')).toBe(true);
  });

  it('counts only the posts the machine is set up with', () => {
    expect(machineWrapsBlank(posts, machines(['flat']), 'x')).toBe(false);
    expect(machineWrapsBlank(posts, machines(['flat', 'along-x']), 'x')).toBe(true);
  });

  it('asks for a post that wraps this way round', () => {
    expect(machineWrapsBlank(posts, machines(['along-x']), 'y')).toBe(false);
    expect(machineWrapsBlank(posts, machines(['both']), 'y')).toBe(true);
  });

  it('says nothing until the posts have loaded', () => {
    expect(machineWrapsBlank(null, machines(['flat']), 'x')).toBe(true);
  });

  it('finds none when no post can wrap', () => {
    expect(machineWrapsBlank([post('flat', null, null)], machines([]), 'x')).toBe(false);
  });
});
