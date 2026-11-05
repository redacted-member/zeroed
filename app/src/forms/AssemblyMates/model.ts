// The Mates list's model: what each mate reads as, the value field each
// kind takes (a gap in the job unit, an angle in degrees, none for
// concentric) and the kinds a mate can change to.

import { MATE_KINDS, type MateKind, type MateRow, type PieceRef } from '../../bridge/assembly';
import type { Units } from '../../bridge/types';
import { assemblyMatesStrings } from '../../lib/strings/assemblyMates';
import { formatLength, parseLength, toMillimetres } from '../../lib/units';

const text = assemblyMatesStrings.mates;

/** The value field a kind takes, or null for none. */
export type MateValueField = 'offset' | 'distance' | 'angle';

export function mateValueField(kind: MateKind): MateValueField | null {
  switch (kind) {
    case 'against':
    case 'flush':
      return 'offset';
    case 'distance':
      return 'distance';
    case 'angle':
      return 'angle';
    case 'concentric':
    case 'joint':
      return null;
  }
}

/** A mate's line in the list: its kind and its two pieces. */
export function mateRowText(row: MateRow, nameOf: (piece: PieceRef) => string): string {
  return text.row(text.kinds[row.kind], nameOf(row.a), nameOf(row.b));
}

function degrees(value: number): string {
  const rounded = Math.round(value * 1000) / 1000;
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

/** A mate's value as the field shows it: a length in the job unit or degrees. */
export function mateValueText(kind: MateKind, value: number, units: Units): string {
  return kind === 'angle' ? degrees(value) : formatLength(value, units);
}

/**
 * The value typed for a kind: millimetres for a gap, degrees for an
 * angle (0 to 180); null when it is not a number or out of range.
 */
export function parseMateValue(kind: MateKind, typed: string, units: Units): number | null {
  const value = parseLength(typed);
  if (value === null || !Number.isFinite(value)) {
    return null;
  }
  if (kind === 'angle') {
    return value >= 0 && value <= 180 ? value : null;
  }
  return toMillimetres(value, units);
}

/**
 * The kinds a mate can change to: round axes stay concentric, a joint mate
 * stays a joint mate, faces take the rest.
 */
export function kindsFor(row: Pick<MateRow, 'kind'>): MateKind[] {
  if (row.kind === 'concentric' || row.kind === 'joint') {
    return [row.kind];
  }
  return MATE_KINDS.filter((k) => k !== 'concentric');
}

/** How many mates are in conflict. */
export function conflictCount(mates: readonly MateRow[]): number {
  return mates.filter((m) => m.state === 'conflict').length;
}

/** What a mate in conflict is off by, in the job unit and degrees. */
export function offText(row: MateRow, units: Units): string {
  return text.off(formatLength(row.distanceError, units), degrees(row.angleError));
}
