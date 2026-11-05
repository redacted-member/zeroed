// Pure state for the Document Variables form: rows of name and value. A
// value is a number or a formula such as {w}/2+1, which stays live: the
// kernel keeps it and works it out again when what it reads changes.

import { evaluate, isVariableName, type Variables } from '../../lib/expression';
import { strings } from '../../lib/formStrings';

export interface VariableRow {
  /** Stable React key. */
  key: number;
  name: string;
  value: string;
}

export interface RowErrors {
  name?: string;
  value?: string;
}

let nextKey = 1;

export function newRow(name = '', value = ''): VariableRow {
  nextKey += 1;
  return { key: nextKey, name, value };
}

/** The rows of the variables by name, a formula shown as typed. */
export function rowsFrom(
  variables: Variables,
  formulas: Readonly<Record<string, string>> = {},
): VariableRow[] {
  return Object.entries(variables)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, value]) =>
      newRow(
        name,
        Object.prototype.hasOwnProperty.call(formulas, name) ? formulas[name] : String(value),
      ),
    );
}

/** How a typed value is defined: a plain number, or the formula as typed. */
export function definitionOf(value: string): number | string {
  const text = value.trim();
  const number = Number(text);
  return text !== '' && Number.isFinite(number) ? number : text;
}

/** The names a value reads, written {name} or [name]. */
export function namesIn(value: string): string[] {
  return [...value.matchAll(/\{([^}]*)\}|\[([^\]]*)\]/g)].map((m) => (m[1] ?? m[2]).trim());
}

/**
 * The value of each row by key, null where it does not read as a number. A
 * value may use the other variables ({w}/2+1), as any calculation box can;
 * rows are worked out in turn until nothing more resolves, so one that
 * refers to itself, round a loop or to a name no row has stays null.
 */
export function rowValues(rows: readonly VariableRow[]): Map<number, number | null> {
  const values = new Map<number, number | null>(rows.map((row) => [row.key, null]));
  const known: Record<string, number> = {};
  let pending = [...rows];
  let progress = true;
  while (pending.length > 0 && progress) {
    progress = false;
    const next: VariableRow[] = [];
    for (const row of pending) {
      const value = evaluate(row.value, known);
      if (value === null) {
        next.push(row);
      } else {
        values.set(row.key, value);
        known[row.name.trim()] = value;
        progress = true;
      }
    }
    pending = next;
  }
  return values;
}

/** Why a row's value does not work out, in plain words: a name no row has,
 * a loop, or not a number. */
function valueProblem(row: VariableRow, rows: readonly VariableRow[]): string {
  const text = strings.documentVariables;
  const byName = new Map(rows.map((r) => [r.name.trim(), r]));
  const name = row.name.trim();
  const unknown = namesIn(row.value).find((used) => !byName.has(used));
  if (unknown !== undefined) {
    return text.unknownName(unknown);
  }
  // Follow what the value reads; coming back to this row is a loop.
  const seen = new Set<string>();
  const stack = namesIn(row.value);
  while (stack.length > 0) {
    const used = stack.pop() ?? '';
    if (used === name) {
      return text.loop;
    }
    if (!seen.has(used)) {
      seen.add(used);
      stack.push(...namesIn(byName.get(used)?.value ?? ''));
    }
  }
  return strings.form.errorNumber;
}

/** Errors per row key; empty when every row is fine. */
export function validateRows(rows: readonly VariableRow[]): Map<number, RowErrors> {
  const text = strings.documentVariables;
  const errors = new Map<number, RowErrors>();
  const seen = new Set<string>();
  const values = rowValues(rows);
  for (const row of rows) {
    const found: RowErrors = {};
    const name = row.name.trim();
    if (!isVariableName(name)) {
      found.name = text.badName;
    } else if (seen.has(name)) {
      found.name = text.duplicate;
    }
    seen.add(name);
    if (values.get(row.key) === null) {
      found.value = valueProblem(row, rows);
    }
    if (found.name !== undefined || found.value !== undefined) {
      errors.set(row.key, found);
    }
  }
  return errors;
}

/** The definitions the rows describe (a number, or a formula kept as
 * typed), or null while any row is wrong. */
export function variablesFrom(
  rows: readonly VariableRow[],
): Record<string, number | string> | null {
  if (validateRows(rows).size > 0) {
    return null;
  }
  const out: Record<string, number | string> = {};
  for (const row of rows) {
    out[row.name.trim()] = definitionOf(row.value);
  }
  return out;
}

/** What a formula row works out to now, for its tooltip; undefined for a
 * plain number or a row that does not work out. */
export function formulaResult(
  row: VariableRow,
  values: Map<number, number | null>,
): string | undefined {
  const value = values.get(row.key);
  if (typeof definitionOf(row.value) === 'number' || value === null || value === undefined) {
    return undefined;
  }
  return strings.documentVariables.worksOut(Math.round(value * 1e6) / 1e6);
}

/** Why a new variable cannot be added yet, or null when it can: a proper
 * name that is not taken, and a value that reads as a number, the other
 * variables included. */
export function newVariableError(
  rows: readonly VariableRow[],
  name: string,
  value: string,
): string | null {
  const text = strings.documentVariables;
  const trimmed = name.trim();
  if (!isVariableName(trimmed)) {
    return text.badName;
  }
  if (rows.some((row) => row.name.trim() === trimmed)) {
    return text.taken;
  }
  const row = newRow(trimmed, value);
  const all = [...rows, row];
  return rowValues(all).get(row.key) === null ? valueProblem(row, all) : null;
}
