// Pure pieces of the Clipart tab: the category tree, filtering, the insert
// scale and where a drop lands in the job.

import type { ClipItem } from '../../bridge/clipart';
import type { JobSetup } from '../../bridge/types';
import {
  jobBounds,
  screenToJob,
  type Camera2D,
  type Point2,
  type Viewport,
} from '../../views/View2D/camera';

/** The category choice that shows every clip. */
export const ALL_CATEGORIES = '';

/** Smallest and largest insert scale, percent. */
export const MIN_SCALE_PERCENT = 1;
export const MAX_SCALE_PERCENT = 10000;

/** Whether a clip filed under `itemCategory` shows when `category` is
 * chosen: the category itself and every subcategory under it. */
export function inCategory(itemCategory: string, category: string): boolean {
  return (
    category === ALL_CATEGORIES ||
    itemCategory === category ||
    itemCategory.startsWith(`${category}/`)
  );
}

/** Clips in `category` and its subcategories (every clip for
 * {@link ALL_CATEGORIES}) whose name, category or tags hold every word of
 * `query`, ignoring case. */
export function filterClips(
  items: readonly ClipItem[],
  category: string,
  query: string,
): ClipItem[] {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w !== '');
  return items.filter((item) => {
    if (!inCategory(item.category, category)) {
      return false;
    }
    const tags = item.tags?.join(' ') ?? '';
    const haystack = `${item.name} ${item.category} ${tags}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  });
}

/** How many clips each category holds, its own only. */
export function categoryCounts(items: readonly ClipItem[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const item of items) {
    counts.set(item.category, (counts.get(item.category) ?? 0) + 1);
  }
  return counts;
}

/** One folder of the category tree. */
export interface CategoryNode {
  /** The full category, `/` between levels. */
  path: string;
  /** The last level's name. */
  name: string;
  /** Clips in it and every folder under it. */
  count: number;
  children: CategoryNode[];
}

/** The categories as a tree of folders, each counting the clips under it,
 * in the order the categories come. */
export function categoryTree(
  categories: readonly string[],
  counts: ReadonlyMap<string, number>,
): CategoryNode[] {
  const roots: CategoryNode[] = [];
  const byPath = new Map<string, CategoryNode>();
  for (const category of categories) {
    const parts = category.split('/');
    let siblings = roots;
    let path = '';
    for (const part of parts) {
      path = path === '' ? part : `${path}/${part}`;
      let node = byPath.get(path);
      if (node === undefined) {
        node = { path, name: part, count: 0, children: [] };
        byPath.set(path, node);
        siblings.push(node);
      }
      node.count += path === category ? (counts.get(category) ?? 0) : 0;
      siblings = node.children;
    }
  }
  const total = (node: CategoryNode): number => {
    node.count += node.children.reduce((sum, child) => sum + total(child), 0);
    return node.count;
  };
  roots.forEach(total);
  return roots;
}

/** Thumbnail sizes the tab offers. */
export type ThumbSize = 'small' | 'medium' | 'large';

/** The category to show after a rescan: the same one while it (or a
 * subcategory under it) still exists. */
export function keptCategory(current: string, categories: readonly string[]): string {
  return current === ALL_CATEGORIES || categories.some((c) => inCategory(c, current))
    ? current
    : ALL_CATEGORIES;
}

/** The insert scale factor for a percent typed by the user, or null when
 * it is not a number in range. */
export function scaleFromPercent(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === '') {
    return null;
  }
  const value = Number(trimmed);
  if (!Number.isFinite(value) || value < MIN_SCALE_PERCENT || value > MAX_SCALE_PERCENT) {
    return null;
  }
  return value / 100;
}

/** The job point under a drop at `client`, given the drop surface's box. */
export function dropPoint(
  camera: Camera2D,
  viewport: Viewport,
  surface: { left: number; top: number },
  client: Point2,
): Point2 {
  return screenToJob(camera, viewport, {
    x: client.x - surface.left,
    y: client.y - surface.top,
  });
}

/** The middle of the job, where a double clicked clip goes. */
export function jobCentre(job: JobSetup): Point2 {
  const bounds = jobBounds(job);
  return { x: (bounds.minX + bounds.maxX) / 2, y: (bounds.minY + bounds.maxY) / 2 };
}
