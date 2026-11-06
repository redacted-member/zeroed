// The Furniture Library's pure parts: the categories down the side, the
// search, the license badge and the default values a card previews.

import type { FurnitureParam, FurnitureValues, TemplateSummary } from '../../bridge/furniture';
import type { Units } from '../../bridge/types';

/** The categories in the order the library lists them. */
export const CATEGORY_ORDER: readonly string[] = [
  'Storage',
  'Tables',
  'Seating',
  'Beds',
  'Desks',
  'Kitchen',
  'Outdoor',
  'Shop',
  'Kids',
  'Decor',
];

/** The pseudo category that lists every template. */
export const ALL = '';

export interface CategoryRow {
  /** The category, or ALL. */
  id: string;
  count: number;
}

/** All first, then each category that holds a template, in the library's order (unknown ones last, by name). */
export function categoriesOf(templates: readonly TemplateSummary[]): CategoryRow[] {
  const counts = new Map<string, number>();
  for (const t of templates) {
    counts.set(t.category, (counts.get(t.category) ?? 0) + 1);
  }
  const rank = (id: string) => {
    const at = CATEGORY_ORDER.indexOf(id);
    return at < 0 ? CATEGORY_ORDER.length : at;
  };
  const ids = [...counts.keys()].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
  return [
    { id: ALL, count: templates.length },
    ...ids.map((id) => ({ id, count: counts.get(id) ?? 0 })),
  ];
}

/** The text a search looks through. */
function haystack(t: TemplateSummary): string {
  return [t.name, t.description, t.category, t.author, ...t.tags].join(' ').toLowerCase();
}

/**
 * The templates in `category` (ALL for every one) whose name, description,
 * category, author or tags hold every word of `query`, by name.
 */
export function filterTemplates(
  templates: readonly TemplateSummary[],
  category: string,
  query: string,
): TemplateSummary[] {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w !== '');
  return templates
    .filter((t) => category === ALL || t.category === category)
    .filter((t) => {
      const text = haystack(t);
      return words.every((w) => text.includes(w));
    })
    .sort((a, b) => a.name.localeCompare(b.name));
}

export type LicenseBadge = 'cc0' | 'ccBy' | 'ccBySa' | 'pd';

/** The badge for an SPDX license id, or null for one the library does not take. */
export function licenseBadge(license: string): LicenseBadge | null {
  switch (license.toUpperCase()) {
    case 'CC0-1.0':
    case 'CC0':
      return 'cc0';
    case 'CC-BY-4.0':
    case 'CC-BY-3.0':
      return 'ccBy';
    case 'CC-BY-SA-4.0':
    case 'CC-BY-SA-3.0':
      return 'ccBySa';
    case 'PD':
    case 'PUBLIC-DOMAIN':
      return 'pd';
    default:
      return null;
  }
}

/** Whether the license asks for the credit line to show (CC BY and BY-SA). */
export function needsCredit(license: string): boolean {
  const badge = licenseBadge(license);
  return badge === 'ccBy' || badge === 'ccBySa';
}

const MM_PER_INCH = 25.4;

/** Board thicknesses as sold in inches, in inches. */
const INCH_BOARDS = [1 / 8, 1 / 4, 3 / 8, 1 / 2, 5 / 8, 3 / 4, 1, 5 / 4, 3 / 2, 2];

/** A length in millimeters as a round inch size: coarser the longer it is. */
function roundInches(mm: number): number {
  const inches = mm / MM_PER_INCH;
  const step = inches >= 6 ? 1 : inches >= 1 ? 1 / 4 : 1 / 16;
  return Math.round(inches / step) * step * MM_PER_INCH;
}

/** A template's metric default as the round inch size nearest it, within its limits. */
function inchDefault(param: FurnitureParam): number | string {
  const value = param.default;
  if (typeof value !== 'number' || value === 0) {
    return value;
  }
  const min = param.min ?? -Infinity;
  const max = param.max ?? Infinity;
  const inside = (mm: number) => mm >= min && mm <= max;
  if (param.kind === 'thickness') {
    const boards = INCH_BOARDS.map((board) => board * MM_PER_INCH).filter(inside);
    const nearest = boards.reduce<number | null>(
      (best, mm) => (best === null || Math.abs(mm - value) < Math.abs(best - value) ? mm : best),
      null,
    );
    return nearest ?? value;
  }
  if (param.kind === 'length') {
    const rounded = roundInches(value);
    return inside(rounded) ? rounded : value;
  }
  return value;
}

/** Each parameter's default value: round inches on an inch job. */
export function defaultValues(
  params: readonly FurnitureParam[],
  units: Units = 'mm',
): FurnitureValues {
  const values: FurnitureValues = {};
  for (const p of params) {
    values[p.key] = units === 'inch' ? inchDefault(p) : p.default;
  }
  return values;
}
