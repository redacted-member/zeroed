// The getting started guide's walkthroughs (guide.json) and the pure parts
// of showing them: which control a step points at, which dock shows it and
// where the step's card goes beside it.

import { TOOL_GROUPS } from '../../shell/LeftPanel/toolGroups';
import type { GuideId } from '../../state/guideStore';
import type { FormId } from '../../state/uiStore';
import content from './guide.json';

export interface GuideStep {
  /** The control the step points at (see `targetSelector`); none centers the card. */
  target?: string;
  /** Clicking the target goes on to the next step. */
  click?: boolean;
  title: string;
  text: string;
}

export interface Guide {
  title: string;
  /** One line on the welcome card. */
  blurb: string;
  steps: GuideStep[];
}

export const GUIDES: Readonly<Record<GuideId, Guide>> = content;

/** Named controls a step can point at, as CSS selectors. */
const TARGETS: Readonly<Record<string, string>> = {
  designTab: '#dock-tab-design',
  sheetsTab: '#dock-tab-sheets',
  layersTab: '#dock-tab-layers',
  componentsTab: '#dock-tab-components',
  clipartTab: '#dock-tab-clipart',
  toolPanel: '#dock-panel',
  toolpaths: '[data-guide="toolpaths"]',
  views: '.view-area__views',
  view2d: '[data-view="2d"]',
  view3d: '[data-view="3d"]',
  assembly: '[data-view="assembly"]',
  form: '.form-host',
};

const TOOL_FORMS = new Map(
  (['design', 'toolpaths'] as const).flatMap((set) =>
    TOOL_GROUPS[set].flatMap((group) =>
      group.tools.flatMap((tool) =>
        tool.form === undefined ? [] : [[tool.id, tool.form] as const],
      ),
    ),
  ),
);

const TOOL_SETS = new Map(
  (['design', 'toolpaths'] as const).flatMap((set) =>
    TOOL_GROUPS[set].flatMap((group) => group.tools.map((tool) => [tool.id, set] as const)),
  ),
);

/** The CSS selector of a step's target, or null when the name is unknown. */
export function targetSelector(target: string): string | null {
  if (target.startsWith('tool:')) {
    const id = target.slice('tool:'.length);
    return TOOL_SETS.has(id) ? `[data-tool="${id}"]` : null;
  }
  return TARGETS[target] ?? null;
}

/** The form a tool target opens, if any: once it is open, the click step is done. */
export function targetForm(target: string): FormId | null {
  return target.startsWith('tool:') ? (TOOL_FORMS.get(target.slice('tool:'.length)) ?? null) : null;
}

/** The dock a target sits in, to show it before pointing at it (null: always shown). */
export function targetDock(target: string): 'design' | 'toolpaths' | null {
  if (target.startsWith('tool:')) {
    return TOOL_SETS.get(target.slice('tool:'.length)) ?? null;
  }
  if (target === 'toolPanel') {
    return 'design';
  }
  return target === 'toolpaths' ? 'toolpaths' : null;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Room kept between the card, its target and the window edge. */
export const CARD_GAP = 12;

/** Down to a whole, even pixel, so the card's edges stay sharp. */
function evenPixel(value: number): number {
  return Math.floor(value / 2) * 2;
}

/**
 * Where the card goes: beside the target (right, then left, then below,
 * then above, the first side with room), kept inside the window; centered
 * when there is no target.
 */
export function placeCard(
  target: Box | null,
  card: { width: number; height: number },
  view: { width: number; height: number },
): { left: number; top: number } {
  const clamp = (value: number, size: number, room: number) =>
    evenPixel(Math.max(CARD_GAP, Math.min(value, room - size - CARD_GAP)));
  if (target === null) {
    return {
      left: clamp((view.width - card.width) / 2, card.width, view.width),
      top: clamp((view.height - card.height) / 2, card.height, view.height),
    };
  }
  const right = target.left + target.width + CARD_GAP;
  const left = target.left - CARD_GAP - card.width;
  const below = target.top + target.height + CARD_GAP;
  const above = target.top - CARD_GAP - card.height;
  if (right + card.width + CARD_GAP <= view.width) {
    return { left: evenPixel(right), top: clamp(target.top, card.height, view.height) };
  }
  if (left >= CARD_GAP) {
    return { left: evenPixel(left), top: clamp(target.top, card.height, view.height) };
  }
  if (below + card.height + CARD_GAP <= view.height) {
    return { left: clamp(target.left, card.width, view.width), top: evenPixel(below) };
  }
  if (above >= CARD_GAP) {
    return { left: clamp(target.left, card.width, view.width), top: evenPixel(above) };
  }
  return placeCard(null, card, view);
}

/** Room between a control and the ring drawn round it. */
export const RING_GAP = 4;

/** The ring round a control: a little larger, but kept inside the window. */
export function ringBox(target: Box, view: { width: number; height: number }): Box {
  const left = Math.max(0, target.left - RING_GAP);
  const top = Math.max(0, target.top - RING_GAP);
  const right = Math.min(view.width, target.left + target.width + RING_GAP);
  const bottom = Math.min(view.height, target.top + target.height + RING_GAP);
  return { left, top, width: right - left, height: bottom - top };
}
