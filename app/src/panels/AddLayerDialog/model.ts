// The Add New Layer dialog's fields and the request they make.

import type { Layer, NewLayerOptions } from '../../bridge/document';

/** The colours new layers cycle through by their position, as the kernel does. */
export const LAYER_COLOURS = [
  '#000000',
  '#d62828',
  '#1f6fd6',
  '#2a9d3f',
  '#b5179e',
  '#e07c00',
  '#008b8b',
] as const;

export interface NewLayerDraft {
  name: string;
  colour: string;
  visible: boolean;
  active: boolean;
}

/** The first "Layer N" name not taken, counting on from the number of layers. */
export function nextLayerName(layers: readonly Pick<Layer, 'name'>[]): string {
  const taken = new Set(layers.map((layer) => layer.name.trim().toLowerCase()));
  let n = layers.length + 1;
  while (taken.has(`layer ${n}`)) {
    n += 1;
  }
  return `Layer ${n}`;
}

/** The dialog's starting values: a free name, the next colour, visible and active. */
export function newLayerDraft(layers: readonly Pick<Layer, 'name'>[]): NewLayerDraft {
  return {
    name: nextLayerName(layers),
    colour: LAYER_COLOURS[layers.length % LAYER_COLOURS.length] ?? '#000000',
    visible: true,
    active: true,
  };
}

export interface NewLayerRequest {
  name: string;
  colour: string;
  options: NewLayerOptions;
}

/** The layers.add request for `draft`, or null while the name is blank. */
export function newLayerRequest(draft: NewLayerDraft, before?: number): NewLayerRequest | null {
  const name = draft.name.trim();
  if (name === '') {
    return null;
  }
  const options: NewLayerOptions = { visible: draft.visible, active: draft.active };
  if (before !== undefined) {
    options.before = before;
  }
  return { name, colour: draft.colour, options };
}
