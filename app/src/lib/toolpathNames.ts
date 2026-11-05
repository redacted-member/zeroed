// The name a new toolpath's form starts with: the strategy's base name and
// the next number not yet used by a toolpath of that base.

import type { StrategyName } from '../bridge/cam';

/** Base names, as the kernel numbers them when a request has no name. */
export const TOOLPATH_BASE_NAMES: Record<StrategyName, string> = {
  profile: 'Profile',
  pocket: 'Pocket',
  drill: 'Drill',
  vgroove: 'V-Groove',
  inlay: 'VGroove Inlay',
  moulding: 'Swept Profile',
  quickEngrave: 'Quick Engrave',
  chamfer: 'Chamfer',
  fluting: 'Fluting',
  prism: 'Prism Carve',
  texture: 'Texture',
  rough3d: '3D Roughing',
  finish3d: '3D Finish',
  laserCutFill: 'Laser Cut',
  laserPicture: 'Laser Picture',
  sketchCarve: 'Sketch Carve',
  laserSketch: 'Laser Sketch',
  threadMilling: 'Thread',
  drillBank: 'Drill Bank',
  straightInlay: 'Inlay',
  photoCarve: 'Photo Carve',
};

/** `base N` with N one more than the highest number any `base N` in `names` uses. */
export function nextToolpathName(base: string, names: readonly string[]): string {
  let highest = 0;
  const prefix = `${base} `;
  for (const name of names) {
    if (!name.startsWith(prefix)) {
      continue;
    }
    // Two tool strategies add the tool after the number: "Pocket 2 [End Mill]".
    const match = /^(\d+)(\s|$)/.exec(name.slice(prefix.length));
    if (match?.[1] !== undefined) {
      highest = Math.max(highest, Number(match[1]));
    }
  }
  return `${base} ${highest + 1}`;
}
