// The getting started guide: whether it shows, which walkthrough runs and
// its step. The guide itself (panels/Guide) loads the first time it opens.

import { create } from 'zustand';

/** The window tour and the three walkthroughs. */
export type GuideId = 'tour' | 'sign' | 'relief' | 'box';

export const GUIDE_IDS: readonly GuideId[] = ['tour', 'sign', 'relief', 'box'];

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
