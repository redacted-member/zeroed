// Quick Keys: values typed straight into the drawing view while a shape is
// being dragged out (or, for the polyline and arc tools, after the first
// point). A plain value or a comma separated pair is positional; a value
// followed by a letter is tagged, for example `5r40x` is a radius of 5 and a
// width of 40. Enter applies the entry. Lengths are in the job's units and
// are converted by the tool that applies them.

import type { Point } from '../bridge/document';

/** Letters that can follow a value. */
export type QuickTag = 'x' | 'y' | 'w' | 'h' | 'r' | 'd' | 's' | 'p' | 'i' | 'a' | 'l';

const TAGS: ReadonlySet<string> = new Set<QuickTag>([
  'x',
  'y',
  'w',
  'h',
  'r',
  'd',
  's',
  'p',
  'i',
  'a',
  'l',
]);

export interface QuickEntry {
  /** Untagged values in typing order (`10` or `10,20`). */
  values: number[];
  /** Tagged values, the last one wins when a letter repeats. */
  tags: Partial<Record<QuickTag, number>>;
}

/** Parses the typed text, or null when it is not a valid entry. */
export function parseQuickEntry(text: string): QuickEntry | null {
  const source = text.trim().toLowerCase();
  if (source === '') {
    return null;
  }
  const entry: QuickEntry = { values: [], tags: {} };
  const token = /\s*(-?(?:\d+\.?\d*|\.\d+))\s*([a-z])?\s*(,)?/y;
  let index = 0;
  while (index < source.length) {
    token.lastIndex = index;
    const match = token.exec(source);
    if (match === null || match[0].length === 0) {
      return null;
    }
    const value = Number(match[1]);
    const tag = match[2];
    if (!Number.isFinite(value)) {
      return null;
    }
    if (tag === undefined) {
      if (match[3] === undefined && token.lastIndex < source.length) {
        return null;
      }
      entry.values.push(value);
    } else if (TAGS.has(tag)) {
      entry.tags[tag as QuickTag] = value;
    } else {
      return null;
    }
    index = token.lastIndex;
  }
  return entry;
}

/** Whether the entry has exactly the given tags (and no positional values). */
export function hasOnlyTags(entry: QuickEntry, ...tags: QuickTag[]): boolean {
  const keys = Object.keys(entry.tags);
  return (
    entry.values.length === 0 && keys.length === tags.length && tags.every((t) => t in entry.tags)
  );
}

export type QuickKeyAction = 'append' | 'erase' | 'clear' | 'commit' | 'ignore';

/**
 * Decides what a key does to the typed text. A digit, a point or a minus
 * sign starts an entry; letters and commas only continue one, so that
 * single letter shortcuts still work while nothing has been typed.
 */
export function quickKeyAction(text: string, key: string): QuickKeyAction {
  const typing = text !== '';
  if (/^[0-9.]$/.test(key)) {
    return 'append';
  }
  if (key === '-') {
    return 'append';
  }
  if (!typing) {
    return 'ignore';
  }
  if (key === ',' || (key.length === 1 && TAGS.has(key.toLowerCase()))) {
    return 'append';
  }
  if (key === 'Backspace') {
    return 'erase';
  }
  if (key === 'Escape') {
    return 'clear';
  }
  if (key === 'Enter') {
    return 'commit';
  }
  return 'ignore';
}

/** The new text after a key, for the append and erase actions. */
export function applyQuickKey(text: string, key: string): string {
  switch (quickKeyAction(text, key)) {
    case 'append':
      return text + key.toLowerCase();
    case 'erase':
      return text.slice(0, -1);
    case 'clear':
    case 'commit':
      return '';
    case 'ignore':
      return text;
  }
}

/** Unit direction from `from` to `to`, or +X when they coincide. */
export function directionOf(from: Point, to: Point): Point {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.hypot(dx, dy);
  return length < 1e-9 ? { x: 1, y: 0 } : { x: dx / length, y: dy / length };
}

/**
 * Next point for tools that step from a last point (polyline, arc, move):
 * `d` goes d along the cursor direction, `X,Y` is an offset from the last
 * point (or, with `pair` 'absolute', the point itself, as Draw Line takes
 * it), `xX yY` is absolute (either may be left out to keep the cursor's
 * value), and `aA lL` is an angle in degrees and a length. Lengths are
 * already in millimetres through `toMm`.
 */
export function quickStepPoint(
  entry: QuickEntry,
  from: Point,
  cursor: Point,
  toMm: (value: number) => number,
  pair: 'offset' | 'absolute' = 'offset',
): Point | null {
  const { values, tags } = entry;
  if (values.length === 1 && Object.keys(tags).length === 0) {
    const [distance] = values;
    if (distance === undefined) {
      return null;
    }
    const dir = directionOf(from, cursor);
    const d = toMm(distance);
    return { x: from.x + dir.x * d, y: from.y + dir.y * d };
  }
  if (values.length === 2 && Object.keys(tags).length === 0) {
    const [dx, dy] = values;
    if (dx === undefined || dy === undefined) {
      return null;
    }
    return pair === 'absolute'
      ? { x: toMm(dx), y: toMm(dy) }
      : { x: from.x + toMm(dx), y: from.y + toMm(dy) };
  }
  if (values.length > 0) {
    return null;
  }
  if (tags.a !== undefined || tags.l !== undefined) {
    if (tags.x !== undefined || tags.y !== undefined) {
      return null;
    }
    const dir = directionOf(from, cursor);
    const angle = tags.a ?? (Math.atan2(dir.y, dir.x) * 180) / Math.PI;
    const length =
      tags.l !== undefined ? toMm(tags.l) : Math.hypot(cursor.x - from.x, cursor.y - from.y);
    const radians = (angle * Math.PI) / 180;
    return { x: from.x + Math.cos(radians) * length, y: from.y + Math.sin(radians) * length };
  }
  if (tags.x !== undefined || tags.y !== undefined) {
    return {
      x: tags.x !== undefined ? toMm(tags.x) : cursor.x,
      y: tags.y !== undefined ? toMm(tags.y) : cursor.y,
    };
  }
  return null;
}
