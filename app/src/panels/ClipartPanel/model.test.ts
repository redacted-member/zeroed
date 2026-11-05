import { describe, expect, it } from 'vitest';

import type { ClipItem } from '../../bridge/clipart';
import { DEFAULT_JOB } from '../../bridge/types';
import {
  ALL_CATEGORIES,
  categoryCounts,
  categoryTree,
  dropPoint,
  filterClips,
  jobCentre,
  keptCategory,
  scaleFromPercent,
} from './model';
import { clipFileName } from './saveClip';

const clip = (name: string, category: string): ClipItem => ({
  name,
  path: `/lib/${category}/${name}.carveclip`,
  category,
  kind: 'relief',
  format: 'clip',
});

const ITEMS = [
  clip('Rosette', 'Samples'),
  clip('Star Dome', 'Samples'),
  clip('Oak Leaf', 'Nature'),
  clip('Maple Leaf', 'Nature'),
];

describe('filterClips', () => {
  it('keeps every clip for all categories and no query', () => {
    expect(filterClips(ITEMS, ALL_CATEGORIES, '')).toHaveLength(4);
  });

  it('keeps one category', () => {
    expect(filterClips(ITEMS, 'Nature', '').map((c) => c.name)).toEqual(['Oak Leaf', 'Maple Leaf']);
  });

  it('matches every word of the query in the name or category, ignoring case', () => {
    expect(filterClips(ITEMS, ALL_CATEGORIES, 'leaf').map((c) => c.name)).toEqual([
      'Oak Leaf',
      'Maple Leaf',
    ]);
    expect(filterClips(ITEMS, ALL_CATEGORIES, '  MAPLE   leaf ').map((c) => c.name)).toEqual([
      'Maple Leaf',
    ]);
    expect(filterClips(ITEMS, ALL_CATEGORIES, 'samples dome').map((c) => c.name)).toEqual([
      'Star Dome',
    ]);
    expect(filterClips(ITEMS, 'Samples', 'leaf')).toEqual([]);
  });
});

describe('categories', () => {
  it('counts clips per category', () => {
    const counts = categoryCounts(ITEMS);
    expect(counts.get('Samples')).toBe(2);
    expect(counts.get('Nature')).toBe(2);
  });

  it('keeps the chosen category only while it still exists', () => {
    expect(keptCategory('Nature', ['Samples', 'Nature'])).toBe('Nature');
    expect(keptCategory('Gone', ['Samples'])).toBe(ALL_CATEGORIES);
    expect(keptCategory(ALL_CATEGORIES, [])).toBe(ALL_CATEGORIES);
  });
});

describe('the library tree', () => {
  const library = [
    { ...clip('Round Dome', 'Shapes/Domes'), tags: ['circle', 'round'] },
    { ...clip('Cone', 'Shapes/Beads and Cones'), tags: ['point'] },
    { ...clip('Ogee', 'Molding and Edge Profiles/Router Edges'), tags: ['profile'] },
    clip('Tulip', 'Flowers'),
  ];
  const categories = [
    'Flowers',
    'Molding and Edge Profiles/Router Edges',
    'Shapes/Beads and Cones',
    'Shapes/Domes',
  ];

  it('nests subcategories under their folder and counts everything below', () => {
    const tree = categoryTree(categories, categoryCounts(library));
    expect(tree.map((n) => [n.name, n.count])).toEqual([
      ['Flowers', 1],
      ['Molding and Edge Profiles', 1],
      ['Shapes', 2],
    ]);
    const shapes = tree[2];
    expect(shapes.path).toBe('Shapes');
    expect(shapes.children.map((n) => [n.path, n.count])).toEqual([
      ['Shapes/Beads and Cones', 1],
      ['Shapes/Domes', 1],
    ]);
  });

  it('shows a folder with every subcategory under it', () => {
    expect(filterClips(library, 'Shapes', '').map((c) => c.name)).toEqual(['Round Dome', 'Cone']);
    expect(filterClips(library, 'Shapes/Domes', '').map((c) => c.name)).toEqual(['Round Dome']);
    expect(filterClips(library, 'Shape', '')).toEqual([]);
  });

  it('searches tags as well as names', () => {
    expect(filterClips(library, ALL_CATEGORIES, 'round').map((c) => c.name)).toEqual([
      'Round Dome',
    ]);
    expect(filterClips(library, ALL_CATEGORIES, 'profile').map((c) => c.name)).toEqual(['Ogee']);
  });

  it('keeps a chosen folder while a subcategory still exists', () => {
    expect(keptCategory('Shapes', categories)).toBe('Shapes');
    expect(keptCategory('Shap', categories)).toBe(ALL_CATEGORIES);
  });
});

describe('scaleFromPercent', () => {
  it('turns a percent into a factor', () => {
    expect(scaleFromPercent('100')).toBe(1);
    expect(scaleFromPercent(' 250 ')).toBe(2.5);
    expect(scaleFromPercent('1')).toBe(0.01);
  });

  it('refuses empty, non numbers and out of range values', () => {
    for (const bad of ['', 'abc', '0', '-5', '0.5', '20000', 'NaN']) {
      expect(scaleFromPercent(bad)).toBeNull();
    }
  });
});

describe('where a clip goes', () => {
  it('maps a drop through the camera, relative to the surface', () => {
    const camera = { centreX: 100, centreY: 50, pxPerMm: 2 };
    const viewport = { width: 400, height: 300 };
    const surface = { left: 10, top: 20 };
    // The surface's centre is the camera centre.
    expect(dropPoint(camera, viewport, surface, { x: 210, y: 170 })).toEqual({ x: 100, y: 50 });
    // 20 px right and 40 px up is 10 mm right and 20 mm up.
    expect(dropPoint(camera, viewport, surface, { x: 230, y: 130 })).toEqual({ x: 110, y: 70 });
  });

  it('puts a double clicked clip in the middle of the job', () => {
    expect(jobCentre({ ...DEFAULT_JOB, width: 300, height: 200, xyOrigin: 'bottomLeft' })).toEqual({
      x: 150,
      y: 100,
    });
    expect(jobCentre({ ...DEFAULT_JOB, xyOrigin: 'centre' })).toEqual({ x: 0, y: 0 });
  });
});

describe('clipFileName', () => {
  it('names the file after the component', () => {
    expect(clipFileName('Rose / 2')).toBe('Rose _ 2.carveclip');
    expect(clipFileName('  ')).toMatch(/\.carveclip$/);
  });
});
