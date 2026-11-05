// Measure model: the lines a measurement shows and its label in the view.

import type { MeasureReply } from '../../bridge/assemblyFit';
import type { Units } from '../../bridge/types';
import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { formatLength, unitSuffix } from '../../lib/units';

/** A length in the job unit, a thousandth of a millimetre being the finest. */
function length(mm: number, units: Units): string {
  return formatLength(Math.abs(mm) < 5e-4 ? 0 : mm, units);
}

/** The label drawn in the view: the face gap for parallel faces, else the distance. */
export function measureLabel(reply: MeasureReply, units: Units): string {
  const mm = reply.faceDistance ?? reply.distance;
  return `${length(mm, units)} ${unitSuffix(units)}`;
}

/** What the form lists for a measurement. */
export function measureLines(reply: MeasureReply, units: Units): string[] {
  const unit = unitSuffix(units);
  const [dx, dy, dz] = reply.delta.map((v) => length(v, units));
  const lines = [text.distance(length(reply.distance, units), dx ?? '', dy ?? '', dz ?? '', unit)];
  lines.push(
    reply.faceDistance === null
      ? text.angle(String(Math.round(reply.angle * 10) / 10))
      : text.faceDistance(length(reply.faceDistance, units), unit),
  );
  lines.push(reply.a.exact && reply.b.exact ? text.measuredExact : text.measuredGrid);
  return lines;
}
