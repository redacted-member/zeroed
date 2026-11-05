// Fit section model: the default target for the job unit, side names, and
// the lines each joint shows. Pure, so it can be tested without the kernel.

import type { PieceRef, Triple } from '../../bridge/assembly';
import type { FitJoint, FitSide, FitStatus } from '../../bridge/assemblyFit';
import type { Units } from '../../bridge/types';
import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { formatDisplay, formatLength, toDisplay, unitSuffix } from '../../lib/units';

/** A side's name: the axis it faces along (+X, -Z), or its direction rounded. */
export function sideName(normal: Triple): string {
  const axes = ['X', 'Y', 'Z'];
  for (let k = 0; k < 3; k += 1) {
    const v = normal[k] ?? 0;
    if (Math.abs(Math.abs(v) - 1) < 0.02) {
      return `${v > 0 ? '+' : '-'}${axes[k] ?? ''}`;
    }
  }
  return `(${normal.map((v) => v.toFixed(2)).join(', ')})`;
}

/** Joints that need a look: anything but OK. */
export function problemCount(joints: readonly FitJoint[]): number {
  return joints.filter((j) => j.status !== 'ok').length;
}

/** Worst first, then in the kernel's order. */
export function jointOrder(joints: readonly FitJoint[]): number[] {
  const rank: Readonly<Record<FitStatus, number>> = {
    interference: 0,
    tight: 1,
    loose: 2,
    ok: 3,
  };
  return joints
    .map((_, i) => i)
    .sort((a, b) => {
      const ja = joints[a];
      const jb = joints[b];
      const d = (ja === undefined ? 4 : rank[ja.status]) - (jb === undefined ? 4 : rank[jb.status]);
      return d === 0 ? a - b : d;
    });
}

/** A gap in the job unit, a hundredth of a millimetre being the finest that shows. */
export function gapText(mm: number, units: Units): string {
  const rounded = units === 'inch' ? mm : Math.round(mm * 1000) / 1000;
  return formatLength(Math.abs(rounded) < 5e-4 ? 0 : rounded, units);
}

/** One side's line: its gap, or that it rests on the other piece. */
export function sideText(side: FitSide, units: Units): string {
  const name = side.round === true ? text.round(sideName(side.normal)) : sideName(side.normal);
  if (!side.fit && Math.abs(side.min) < 0.001 && Math.abs(side.max) < 0.001) {
    return text.rests(name);
  }
  const min = gapText(side.min, units);
  const max = gapText(side.max, units);
  return text.gap(name, min, min === max ? null : max);
}

/** What a joint's row says. */
export interface JointLines {
  title: string;
  status: string;
  sides: string[];
  overlap: string | null;
  source: string;
}

export function jointLines(
  joint: FitJoint,
  nameOf: (ref: PieceRef) => string,
  units: Units,
): JointLines {
  const unit = unitSuffix(units);
  const volume = toDisplay(toDisplay(toDisplay(joint.overlapVolume, units), units), units);
  return {
    title: text.joint(nameOf(joint.a), nameOf(joint.b)),
    status: text.status[joint.status],
    sides: joint.sides.map((side) => sideText(side, units)),
    overlap:
      joint.overlapDepth > 0 || joint.overlapVolume > 0
        ? text.overlap(gapText(joint.overlapDepth, units), formatDisplay(volume, units), unit)
        : null,
    source:
      joint.source === 'exact' ? text.exact : text.grid(formatLength(joint.cell ?? 0, units), unit),
  };
}
