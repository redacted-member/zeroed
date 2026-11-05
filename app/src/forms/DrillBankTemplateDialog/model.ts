// Pure rules of the Drill Bank Template dialog: the grid of drill positions,
// the reference (central) drill and each drill's number and diameter.
// Lengths are millimetres.

import type { BankDrill, DrillBankTemplate } from '../../bridge/cam';

/** Most positions along either side of a bank. */
export const MAX_SIDE = 20;

/** A new bank: nine 5 mm drills in a row on the 32 mm system, the first
 * the reference, as the kernel's default. */
export function defaultTemplate(): DrillBankTemplate {
  return {
    horizontal: 9,
    vertical: 1,
    pitch: 32,
    reference: [0, 0],
    drills: Array.from({ length: 9 }, (_, i) => ({ number: i + 1, diameter: 5 })),
  };
}

/** The bank with `horizontal` by `vertical` positions: drills keep their
 * column and row, new positions get the next free numbers and the last
 * diameter, and the reference stays inside. */
export function resized(
  template: DrillBankTemplate,
  horizontal: number,
  vertical: number,
): DrillBankTemplate {
  const h = Math.min(Math.max(Math.round(horizontal), 1), MAX_SIDE);
  const v = Math.min(Math.max(Math.round(vertical), 1), MAX_SIDE);
  const keptAt = (column: number, row: number): BankDrill | undefined =>
    column < template.horizontal && row < template.vertical
      ? template.drills[row * template.horizontal + column]
      : undefined;
  // Numbers of the drills that stay; the dropped ones are free again.
  const used = new Set<number>();
  for (let row = 0; row < v; row += 1) {
    for (let column = 0; column < h; column += 1) {
      const number = keptAt(column, row)?.number ?? 0;
      if (number > 0) {
        used.add(number);
      }
    }
  }
  let next = 1;
  const fresh = (): number => {
    while (used.has(next)) {
      next += 1;
    }
    used.add(next);
    return next;
  };
  const diameter = template.drills[template.drills.length - 1]?.diameter ?? 5;
  const drills: BankDrill[] = [];
  for (let row = 0; row < v; row += 1) {
    for (let column = 0; column < h; column += 1) {
      drills.push(keptAt(column, row) ?? { number: fresh(), diameter });
    }
  }
  return {
    ...template,
    horizontal: h,
    vertical: v,
    reference: [Math.min(template.reference[0], h - 1), Math.min(template.reference[1], v - 1)],
    drills,
  };
}

/** Index of position (`column`, `row`). */
export function drillIndex(template: DrillBankTemplate, column: number, row: number): number {
  return row * template.horizontal + column;
}

/** Column and row of position `index`. */
export function drillPosition(template: DrillBankTemplate, index: number): [number, number] {
  return [index % template.horizontal, Math.floor(index / template.horizontal)];
}

/** The bank with drill `index` changed. */
export function withDrill(
  template: DrillBankTemplate,
  index: number,
  drill: Partial<BankDrill>,
): DrillBankTemplate {
  return {
    ...template,
    drills: template.drills.map((d, i) => (i === index ? { ...d, ...drill } : d)),
  };
}

/** The bank with its reference (central) drill at position `index`. */
export function withReference(template: DrillBankTemplate, index: number): DrillBankTemplate {
  return { ...template, reference: drillPosition(template, index) };
}

/** Whether position `index` is the reference drill. */
export function isReference(template: DrillBankTemplate, index: number): boolean {
  const [column, row] = drillPosition(template, index);
  return template.reference[0] === column && template.reference[1] === row;
}

/** What is wrong with the bank, or null: the kernel refuses the same. */
export type TemplateProblem = 'pitch' | 'noDrills' | 'diameter' | 'duplicate';

export function templateProblem(template: DrillBankTemplate): TemplateProblem | null {
  if (!(template.pitch > 0)) {
    return 'pitch';
  }
  const fitted = template.drills.filter((d) => d.number > 0);
  if (fitted.length === 0) {
    return 'noDrills';
  }
  if (fitted.some((d) => !(d.diameter > 0))) {
    return 'diameter';
  }
  if (new Set(fitted.map((d) => d.number)).size !== fitted.length) {
    return 'duplicate';
  }
  return null;
}
