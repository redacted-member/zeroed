// Whether the machine in use can write a rotary job, for the Toolpaths panel.

import type { PostSummary } from '../../bridge/cam';
import type { MachinePrefs } from '../../bridge/prefs';
import type { RotaryAxis } from '../../bridge/types';

/**
 * True when the active machine has a post that wraps a blank lying along
 * `axis`. A machine with no posts of its own is offered all of them. Before the
 * posts have loaded (null) there is nothing to warn about yet.
 */
export function machineWrapsBlank(
  posts: readonly PostSummary[] | null,
  machines: MachinePrefs,
  axis: RotaryAxis,
): boolean {
  if (posts === null) {
    return true;
  }
  const own = machines.list.find((machine) => machine.name === machines.active)?.posts ?? [];
  return posts
    .filter((post) => own.length === 0 || own.includes(post.id))
    .some((post) => (axis === 'x' ? post.rotaryWrapY : post.rotaryWrapX));
}
