// The document variables (doc.variables.get / set), mirrored for numeric
// fields to use in expressions such as [width] / 2. A variable defined by a
// formula keeps it; the kernel works its value out again when what it reads
// changes.

import { create } from 'zustand';

import { api } from '../bridge/api';
import type { Unsubscribe } from '../bridge/types';
import { setConstantSource, setVariableSource, type Variables } from '../lib/expression';
import { toDisplay } from '../lib/units';
import { useDocumentStore } from './documentStore';

/** How each variable is defined: its formula, or its value. */
export type Definitions = Record<string, number | string>;

interface VariablesState {
  /** Every variable's value. */
  variables: Variables;
  /** The formula of each variable defined by one. */
  formulas: Readonly<Record<string, string>>;
  /** Reads the variables from the kernel. */
  load: () => Promise<void>;
  /** Replaces every variable; one undo step. */
  save: (definitions: Definitions) => Promise<void>;
}

/** The definitions of the variables: the formula where there is one. */
export function definitionsOf(
  variables: Variables,
  formulas: Readonly<Record<string, string>>,
): Definitions {
  const out: Definitions = {};
  for (const [name, value] of Object.entries(variables)) {
    out[name] = Object.prototype.hasOwnProperty.call(formulas, name) ? formulas[name] : value;
  }
  return out;
}

export const useVariablesStore = create<VariablesState>((set, get) => ({
  variables: {},
  formulas: {},
  load: async () => {
    const reply = await api.doc.variables();
    set({ variables: reply.variables, formulas: reply.formulas ?? {} });
  },
  save: async (definitions) => {
    await api.doc.setVariables(definitions);
    await get().load();
    await useDocumentStore.getState().refresh();
  },
}));

setVariableSource(() => useVariablesStore.getState().variables);

// The calculation letters (W, H, T and feet) read the material in the job's unit.
setConstantSource(() => {
  const job = useDocumentStore.getState().job;
  return {
    width: toDisplay(job.width, job.units),
    height: toDisplay(job.height, job.units),
    thickness: toDisplay(job.thickness, job.units),
    foot: job.units === 'inch' ? 12 : 304.8,
  };
});

/** Reloads the variables whenever the document changes (open, undo, new). */
export function watchVariables(): Unsubscribe {
  return useDocumentStore.subscribe((doc, previous) => {
    if (doc.revision !== previous.revision && doc.kernelStatus === 'ready') {
      void useVariablesStore
        .getState()
        .load()
        .catch(() => undefined);
    }
  });
}
