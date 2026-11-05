import { describe, expect, it } from 'vitest';

import {
  formulaResult,
  newRow,
  newVariableError,
  rowValues,
  rowsFrom,
  validateRows,
  variablesFrom,
} from './model';

describe('document variables form', () => {
  it('lists variables by name and reads them back', () => {
    const rows = rowsFrom({ width: 300, depth: 6 });
    expect(rows.map((r) => r.name)).toEqual(['depth', 'width']);
    expect(variablesFrom(rows)).toEqual({ depth: 6, width: 300 });
  });

  it('refuses bad and repeated names and values that are not numbers', () => {
    const rows = [newRow('2x', '1'), newRow('a', 'x'), newRow('a', '2')];
    const errors = validateRows(rows);
    expect(errors.get(rows[0].key)?.name).toBeDefined();
    expect(errors.get(rows[1].key)?.value).toBeDefined();
    expect(errors.get(rows[2].key)?.name).toBeDefined();
    expect(variablesFrom(rows)).toBeNull();
  });

  it('keeps a calculation as typed, as a formula', () => {
    expect(variablesFrom([newRow(' half ', '300 / 2'), newRow('n', ' 4 ')])).toEqual({
      half: '300 / 2',
      n: 4,
    });
  });

  it('shows a formula as typed and what it works out to', () => {
    const rows = rowsFrom({ w: 10, d: 6 }, { d: '{w}/2+1' });
    expect(rows.map((r) => r.value)).toEqual(['{w}/2+1', '10']);
    const values = rowValues(rows);
    expect(formulaResult(rows[0], values)).toContain('6');
    expect(formulaResult(rows[1], values)).toBeUndefined();
  });

  it('checks a new variable before it is added', () => {
    const rows = rowsFrom({ width: 300 });
    expect(newVariableError(rows, 'depth', '6')).toBeNull();
    expect(newVariableError(rows, 'width', '6')).toBeDefined();
    expect(newVariableError(rows, '9x', '6')).not.toBeNull();
    expect(newVariableError(rows, 'depth', 'x')).not.toBeNull();
  });

  it('lets a value use the other variables, in any order, but not round a loop', () => {
    const rows = [newRow('d', '{w}/2+1'), newRow('w', '42')];
    expect(validateRows(rows).size).toBe(0);
    expect(variablesFrom(rows)).toEqual({ d: '{w}/2+1', w: 42 });
    expect(newVariableError(rowsFrom({ w: 42 }), 'h', '{w}*2')).toBeNull();
    expect(newVariableError(rowsFrom({ w: 42 }), 'h', '{nope}*2')).not.toBeNull();
    const loop = [newRow('a', '{b}+1'), newRow('b', '{a}+1'), newRow('c', '{c}')];
    expect([...validateRows(loop).values()].map((e) => e.value !== undefined)).toEqual([
      true,
      true,
      true,
    ]);
    expect(variablesFrom(loop)).toBeNull();
  });

  it('says plainly why a formula is refused', () => {
    const loop = [newRow('a', '{b}+1'), newRow('b', '{a}+1'), newRow('c', '{nope}')];
    const errors = validateRows(loop);
    expect(errors.get(loop[0].key)?.value).toMatch(/loop/);
    expect(errors.get(loop[1].key)?.value).toMatch(/loop/);
    expect(errors.get(loop[2].key)?.value).toBe('There is no variable named nope.');
    expect(newVariableError(rowsFrom({ w: 1 }), 'h', '{h}*2')).toMatch(/loop/);
  });
});
