// The depth check after a job size change: a message naming the toolpaths
// that now cut further below the bottom of their material than they were set
// to (a through cut keeps its own allowance), and which job changes can
// change depths at all.

import type { DepthCheckReply } from '../bridge/cam';
import type { JobSetup, Units } from '../bridge/types';
import { strings } from './strings';
import { formatLength, unitSuffix } from './units';

/** The message for a depth check, or null when every toolpath fits. */
export function depthWarning(reply: DepthCheckReply, units: Units): string | null {
  if (reply.toolpaths.length === 0) {
    return null;
  }
  const suffix = unitSuffix(units);
  const list = reply.toolpaths
    .map((t) =>
      strings.toolpathShell.tooDeepItem(
        t.name,
        `${formatLength(t.depth, units)} ${suffix}`,
        `${formatLength(t.thickness, units)} ${suffix}`,
      ),
    )
    .join('; ');
  return strings.toolpathShell.tooDeep(reply.toolpaths.length, list, reply.all);
}

/** Whether a change from `before` to `after` can make any toolpath cut
 * deeper into or through its material: thickness, Z zero, the job kind, the
 * bottom side's Z zero and the model's place in the material. A change of
 * size, XY origin, units or look alone cannot, so it is not checked. */
export function depthsMayChange(before: JobSetup, after: JobSetup): boolean {
  return (
    before.thickness !== after.thickness ||
    before.zZero !== after.zZero ||
    before.kind !== after.kind ||
    before.doubleSided?.bottomZZero !== after.doubleSided?.bottomZZero ||
    before.machine.modelGap !== after.machine.modelGap ||
    (before.machine.gapBelow ?? false) !== (after.machine.gapBelow ?? false) ||
    before.rotary.diameter !== after.rotary.diameter ||
    before.rotary.zZero !== after.rotary.zZero
  );
}
