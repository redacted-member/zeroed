// The gadgets the kernel knows and the one the gadget form is showing.

import { create } from 'zustand';

import { api } from '../bridge/api';
import type { GadgetSummary } from '../bridge/gadgets';

interface GadgetState {
  gadgets: GadgetSummary[];
  loaded: boolean;
  /** Why the last list request failed, or null. */
  failure: string | null;
  /** Name of the gadget the form runs. */
  current: string | null;
  load: () => Promise<void>;
  choose: (name: string) => void;
}

export const useGadgetStore = create<GadgetState>()((set) => ({
  gadgets: [],
  loaded: false,
  failure: null,
  current: null,
  load: async () => {
    try {
      const reply = await api.gadgets.list();
      set({ gadgets: reply.gadgets, loaded: true, failure: null });
    } catch (error) {
      set({ loaded: true, failure: error instanceof Error ? error.message : String(error) });
    }
  },
  choose: (current) => set({ current }),
}));
