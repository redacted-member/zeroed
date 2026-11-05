// Assembly View form model: piece names, the placement fields in the job
// unit, the clearance check's lines and the cut list CSV. Pure, so it can
// be tested without the kernel.

import { pieceKey } from '../../bridge/assembly';
import type {
  AddableComponent,
  PieceCutRow,
  PieceIssue,
  PieceNaming,
  PieceRef,
  PieceRow,
  Triple,
} from '../../bridge/assembly';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import {
  formatDisplay,
  formatLength,
  parseLength,
  toDisplay,
  toMillimetres,
} from '../../lib/units';

const text = strings.assembly;

/**
 * The list name of a piece: the named vector it was cut from, else its
 * sheet's name, with the part number when the sheet has more than one; a
 * sheet's raw model is named after the sheet as its model; a component
 * piece by its component's name.
 */
export function pieceLabel(piece: PieceNaming): string {
  if (piece.component !== undefined && piece.component !== null) {
    const name = piece.componentName?.trim() ?? '';
    return name === '' ? text.componentPiece(piece.component) : name;
  }
  if (piece.model) {
    return text.modelPiece(piece.sheetName);
  }
  const vector = piece.vectorName?.trim() ?? '';
  if (vector !== '') {
    return vector;
  }
  return piece.parts > 1
    ? text.partPiece(piece.sheetName, piece.part)
    : text.sheetPiece(piece.sheetName);
}

/** The add list's entries: each component's label, with its sheet when the job has several. */
export function componentOptions(
  components: readonly AddableComponent[],
): { id: number; label: string }[] {
  const several = new Set(components.map((c) => c.sheet)).size > 1;
  return components.map((c) => ({
    id: c.id,
    label: text.componentOption(
      c.name.trim() === '' ? text.componentPiece(c.id) : c.name,
      several ? c.sheetName : null,
    ),
  }));
}

/** What the add list says when there is nothing to add: that the job has
 * no components, or that every one is a piece already. */
export function noComponentNote(jobComponents: number): string {
  return jobComponents === 0 ? text.jobHasNoComponents : text.noComponents;
}

/** The turns a mate offers about the face normal, degrees. */
export const MATE_TURNS = [0, 90, 180, 270] as const;

/** The sections of the form that need a piece to work on. */
export const PIECE_SECTIONS = [
  'mates',
  'placement',
  'matePieces',
  'fit',
  'section',
  'measure',
  'cutList',
] as const;
export type PieceSection = (typeof PIECE_SECTIONS)[number];

/**
 * Which piece sections the form shows: none before the first piece, when
 * the hint, the Pieces list and Add Component are all a user can act on.
 */
export function shownSections(pieceCount: number): readonly PieceSection[] {
  return pieceCount > 0 ? PIECE_SECTIONS : [];
}

/** The placement fields: position in the job unit, rotation in degrees. */
export interface PlacementText {
  position: [string, string, string];
  rotation: [string, string, string];
}

function degrees(value: number): string {
  const rounded = Math.round(value * 1000) / 1000;
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

export function placementText(
  piece: Pick<PieceRow, 'position' | 'rotation'>,
  units: Units,
): PlacementText {
  const [px, py, pz] = piece.position;
  const [rx, ry, rz] = piece.rotation;
  return {
    position: [formatLength(px, units), formatLength(py, units), formatLength(pz, units)],
    rotation: [degrees(rx), degrees(ry), degrees(rz)],
  };
}

export type ParsedPlacement =
  { ok: true; position: Triple; rotation: Triple } | { ok: false; bad: number[] };

/** Reads the six fields; `bad` lists the ones that are not numbers (0 to 5). */
export function parsePlacement(fields: PlacementText, units: Units): ParsedPlacement {
  const values = [...fields.position, ...fields.rotation].map((t) => parseLength(t));
  const bad = values.flatMap((v, i) => (v === null || !Number.isFinite(v) ? [i] : []));
  if (bad.length > 0) {
    return { ok: false, bad };
  }
  const n = (i: number) => values[i] ?? 0;
  return {
    ok: true,
    position: [toMillimetres(n(0), units), toMillimetres(n(1), units), toMillimetres(n(2), units)],
    rotation: [n(3), n(4), n(5)],
  };
}

/** A volume in the job unit cubed. */
export function volumeText(mm3: number, units: Units): string {
  const side = toDisplay(1, units);
  const value = mm3 * side * side * side;
  return units === 'inch' ? value.toFixed(3) : value.toFixed(0);
}

/** One line of the clearance check. */
export function issueText(
  issue: PieceIssue,
  nameOf: (piece: PieceRef) => string,
  clearance: number,
  units: Units,
): string {
  const unit = units === 'inch' ? 'inch' : 'mm';
  return issue.overlap > 0
    ? text.overlap(nameOf(issue.a), nameOf(issue.b), volumeText(issue.overlap, units), `${unit}³`)
    : text.tooClose(nameOf(issue.a), nameOf(issue.b), formatLength(clearance, units), unit);
}

/** Quotes a CSV field when it holds a comma, a quote or a line break. */
export function csvField(value: string): string {
  return /[",\r\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

function cutLength(mm: number | null, units: Units): string {
  return mm === null ? '' : formatDisplay(toDisplay(mm, units), units);
}

/** Identical pieces of the cut list as one line with their count. */
export interface CutGroup {
  key: string;
  /** The names of the pieces, each once. */
  label: string;
  count: number;
  length: number | null;
  width: number | null;
  thickness: number;
}

/**
 * The cut list with identical pieces together: the same length, width,
 * thickness and outline area (to a hundredth of a millimetre and a square
 * millimetre for exact sizes, half a millimetre and 25 square millimetres
 * on the grid), in the order each first turns up.
 */
export function groupCutList(rows: readonly PieceCutRow[]): CutGroup[] {
  const groups: CutGroup[] = [];
  const names = new Map<string, string[]>();
  for (const row of rows) {
    const exact = row.exact === true;
    const step = exact ? 0.01 : 0.5;
    const size = (v: number | null) => (v === null ? 'none' : String(Math.round(v / step)));
    const area = row.area === undefined ? 'none' : String(Math.round(row.area / (exact ? 1 : 25)));
    const key =
      row.length === null
        ? `piece ${pieceKey(row)}`
        : [exact, size(row.length), size(row.width), size(row.thickness), area].join('|');
    const group = groups.find((g) => g.key === key);
    const label = pieceLabel(row);
    if (group === undefined) {
      groups.push({
        key,
        label,
        count: 1,
        length: row.length,
        width: row.width,
        thickness: row.thickness,
      });
      names.set(key, [label]);
    } else {
      group.count += 1;
      const list = names.get(key) ?? [];
      if (!list.includes(label)) {
        list.push(label);
        group.label = list.join(', ');
      }
      names.set(key, list);
    }
  }
  return groups;
}

/** The cut list as CSV in the job unit, a header line first, identical pieces counted. */
export function cutListCsv(rows: readonly PieceCutRow[], units: Units): string {
  const lines = [text.csvHeader.join(',')];
  for (const group of groupCutList(rows)) {
    lines.push(
      [
        group.label,
        String(group.count),
        cutLength(group.length, units),
        cutLength(group.width, units),
        cutLength(group.thickness, units),
      ]
        .map(csvField)
        .join(','),
    );
  }
  return `${lines.join('\r\n')}\r\n`;
}

/** The default cut list file name for a job path. */
export function cutListName(jobPath: string | null): string {
  const base =
    jobPath
      ?.split(/[/\\]/)
      .pop()
      ?.replace(/\.[^.]*$/, '') ?? '';
  return `${base === '' ? text.cutList : `${base} ${text.cutList}`}.csv`;
}
