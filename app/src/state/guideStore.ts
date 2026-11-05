// The getting started guide: whether it shows, which walkthrough runs and
// its step. The guide itself (panels/Guide) loads the first time it opens.

import { create } from 'zustand';

/** The two to begin with, then a walkthrough for each kind of project. */
export const START_GUIDE_IDS = ['tour', 'machine'] as const;

export const PROJECT_GUIDE_IDS = [
  'cutout',
  'sign',
  'relief',
  'rotary',
  'trace',
  'twoSided',
  'nesting',
  'box',
  'furniture',
  'laser',
] as const;

export type GuideId = (typeof START_GUIDE_IDS)[number] | (typeof PROJECT_GUIDE_IDS)[number];

export const GUIDE_IDS: readonly GuideId[] = [...START_GUIDE_IDS, ...PROJECT_GUIDE_IDS];

interface GuideState {
  open: boolean;
  /** The walkthrough running, null while the welcome card shows. */
  guide: GuideId | null;
  step: number;
  /** Opens the welcome card, where a walkthrough is picked. */
  showWelcome: () => void;
  /** Starts a walkthrough at its first step. */
  start: (guide: GuideId) => void;
  /** Goes to a step of the running walkthrough. */
  goTo: (step: number) => void;
  close: () => void;
}

export const useGuideStore = create<GuideState>()((set) => ({
  open: false,
  guide: null,
  step: 0,
  showWelcome: () => set({ open: true, guide: null, step: 0 }),
  start: (guide) => set({ open: true, guide, step: 0 }),
  goTo: (step) => set({ step: Math.max(0, step) }),
  close: () => set({ open: false, guide: null, step: 0 }),
}));
