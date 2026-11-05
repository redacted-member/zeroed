// Pure state for the Emboss Component form: how much of its height the
// selected component keeps and how much the kept detail is smoothed.

import type { ComponentSummary } from '../../bridge/model';
import type { EmbossComponentArgs } from '../../bridge/modelRefine';

export interface EmbossComponentState {
  /** Height kept, 1 to 100 percent. */
  scale: number;
  /** 0 to 100 percent. */
  detailSmoothing: number;
}

export function initialEmbossComponentState(): EmbossComponentState {
  return { scale: 10, detailSmoothing: 0 };
}

function percent(value: number, min: number): number {
  return Math.min(100, Math.max(min, Math.round(value)));
}

/** The model.embossComponent arguments, or null without a component. */
export function embossArgs(
  state: EmbossComponentState,
  id: number | null,
  replace: number | undefined,
): EmbossComponentArgs | null {
  if (id === null) {
    return null;
  }
  return {
    id,
    scale: percent(state.scale, 1),
    detailSmoothing: percent(state.detailSmoothing, 0),
    ...(replace === undefined ? {} : { replace }),
  };
}

/** The component's top after embossing, from its top now (mm). */
export function embossedTop(maxHeight: number | null, state: EmbossComponentState): number | null {
  return maxHeight === null ? null : (maxHeight * percent(state.scale, 1)) / 100;
}

/** Components lower than this share of their smaller side are low relief. */
const LOW_RELIEF = 0.1;

/** Whether embossing may not suit the component: the tool is meant for
 * full 3D (high relief) models, and one not much taller than a tenth of
 * its width is already a low relief. */
export function lowRelief(component: ComponentSummary): boolean {
  const { bbox, maxHeight } = component;
  if (bbox === undefined || maxHeight === null) {
    return false;
  }
  const side = Math.min(bbox[2] - bbox[0], bbox[3] - bbox[1]);
  return side > 0 && Math.abs(maxHeight) < side * LOW_RELIEF;
}
