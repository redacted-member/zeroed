// Job templates: which template dialog is open, the list the kernel reports,
// and the new-from-template and save-as-template actions.

import { create } from 'zustand';

import { api } from '../bridge/api';
import type { TemplateList, TemplateSaveArgs } from '../bridge/templates';
import { useDocumentStore } from './documentStore';
import { useViewStore } from './viewStore';

export type TemplateDialogKind = 'new' | 'save';

interface TemplateState {
  dialog: TemplateDialogKind | null;
  list: TemplateList | null;
  loading: boolean;
  error: string | null;
  openNew: () => void;
  openSave: () => void;
  close: () => void;
  load: () => Promise<void>;
  /** Opens a template as a new untitled job. True when it opened. */
  newFrom: (path: string) => Promise<boolean>;
  /** Saves the job as a template. True when it was written. */
  save: (args: TemplateSaveArgs) => Promise<boolean>;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const useTemplateStore = create<TemplateState>()((set) => ({
  dialog: null,
  list: null,
  loading: false,
  error: null,
  openNew: () => set({ dialog: 'new', error: null }),
  openSave: () => set({ dialog: 'save', error: null }),
  close: () => set({ dialog: null, error: null }),
  load: async () => {
    set({ loading: true });
    try {
      set({ list: await api.templates.list(), error: null });
    } catch (error) {
      set({ error: message(error) });
    } finally {
      set({ loading: false });
    }
  },
  newFrom: async (path) => {
    try {
      await api.templates.newFrom(path);
      const documents = useDocumentStore.getState();
      await documents.refresh();
      documents.jobReplaced();
      useViewStore.getState().zoomToJob(useDocumentStore.getState().job);
      set({ error: null });
      return true;
    } catch (error) {
      set({ error: message(error) });
      return false;
    }
  },
  save: async (args) => {
    try {
      await api.templates.save(args);
      set({ error: null, list: null });
      return true;
    } catch (error) {
      set({ error: message(error) });
      return false;
    }
  },
}));
