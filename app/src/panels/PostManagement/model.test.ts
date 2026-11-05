import { describe, expect, it } from 'vitest';

import type { PostSummary } from '../../bridge/cam';
import { features, isCustom, lastChanged, visiblePosts } from './model';

const post = (id: string, name: string, extra: Partial<PostSummary> = {}): PostSummary => ({
  id,
  name,
  fileExtension: 'nc',
  units: 'mm',
  arcs: true,
  ...extra,
});

describe('post-processor management rules', () => {
  const posts = [
    post('grbl_mm', 'GRBL (mm)', { fileExtension: 'gcode' }),
    post('mach3_mm', 'Mach3 (mm)', { fileExtension: 'tap' }),
    post('mine', 'Mine', { sourcePath: '/posts/mine.pp', arcs: false, jet: true }),
  ];

  it('lists the user posts first, then by name', () => {
    expect(visiblePosts(posts, '').map((p) => p.id)).toEqual(['mine', 'grbl_mm', 'mach3_mm']);
    expect(isCustom(posts[2] as PostSummary)).toBe(true);
    expect(isCustom(posts[0] as PostSummary)).toBe(false);
  });

  it('filters on every typed word across name, id and extension', () => {
    expect(visiblePosts(posts, 'tap').map((p) => p.id)).toEqual(['mach3_mm']);
    expect(visiblePosts(posts, 'grbl gcode').map((p) => p.id)).toEqual(['grbl_mm']);
    expect(visiblePosts(posts, 'nothing')).toEqual([]);
  });

  it('names what a post supports', () => {
    expect(features(posts[0] as PostSummary)).toEqual(['arcs']);
    expect(features(posts[2] as PostSummary)).toEqual(['laser']);
  });
});

describe('post change logs', () => {
  it('reads the newest dated row as the last change', () => {
    expect(lastChanged([])).toBeNull();
    expect(
      lastChanged([
        { who: 'CarveStudio', when: '2026-09-20', what: 'First version.' },
        { who: 'Sam', when: '2026-09-24', what: 'Helical arcs.' },
        { who: '', when: '', what: 'A note without a date.' },
      ]),
    ).toBe('2026-09-24');
  });
});
