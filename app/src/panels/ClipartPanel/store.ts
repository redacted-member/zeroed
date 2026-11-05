// State of the Clipart tab, shared by the panel and the 2D view's drop
// handler: the scanned library, the filters and the insert scale.

import { create } from 'zustand';

import { api } from '../../bridge/api';
import type { ClipItem } from '../../bridge/clipart';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { ALL_CATEGORIES, keptCategory, scaleFromPercent, type ThumbSize } from './model';

interface ClipartState {
  items: ClipItem[];
  categories: string[];
  userDir: string | null;
  loading: boolean;
  /** Set once a scan has answered. */
  loaded: boolean;
  error: string | null;
  /** Message after the last insert or save. */
  status: string | null;
  category: string;
  query: string;
  /** Insert scale as typed, percent. */
  scaleText: string;
  /** How big the thumbnails are drawn. */
  thumbSize: ThumbSize;
  /** Category folders shown open in the tree. */
  expanded: string[];
  setThumbSize: (size: ThumbSize) => void;
  /** Chooses a category folder: a new choice opens it, choosing the open
   * one again closes it. */
  chooseFolder: (path: string) => void;
  setCategory: (category: string) => void;
  setQuery: (query: string) => void;
  setScaleText: (text: string) => void;
  scan: () => Promise<void>;
  /** Inserts a clip centred on a job point at the current scale, or at
   * `scale` when given. Answers whether it went in. */
  insert: (
    item: Pick<ClipItem, 'path' | 'name'>,
    x: number,
    y: number,
    scale?: number,
  ) => Promise<boolean>;
  setStatus: (status: string | null) => void;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const useClipartStore = create<ClipartState>()((set, get) => ({
  items: [],
  categories: [],
  userDir: null,
  loading: false,
  loaded: false,
  error: null,
  status: null,
  category: ALL_CATEGORIES,
  query: '',
  scaleText: '100',
  thumbSize: 'medium',
  expanded: [],
  setThumbSize: (thumbSize) => set({ thumbSize }),
  chooseFolder: (path) => {
    const { category, expanded } = get();
    const open = expanded.includes(path);
    if (category !== path) {
      set({ category: path, expanded: open ? expanded : [...expanded, path] });
    } else {
      set({ expanded: open ? expanded.filter((p) => p !== path) : [...expanded, path] });
    }
  },
  setCategory: (category) => set({ category }),
  setQuery: (query) => set({ query }),
  setScaleText: (scaleText) => set({ scaleText }),
  setStatus: (status) => set({ status }),
  scan: async () => {
    set({ loading: true, error: null });
    try {
      const reply = await api.clipart.scan();
      set({
        items: reply.items,
        categories: reply.categories,
        userDir: reply.userDir,
        category: keptCategory(get().category, reply.categories),
        loaded: true,
      });
    } catch (error) {
      set({ error: message(error), loaded: true });
    } finally {
      set({ loading: false });
    }
  },
  insert: async (item, x, y, scale) => {
    const factor = scale ?? scaleFromPercent(get().scaleText);
    if (factor === null) {
      set({ error: strings.clipart.badScale });
      return false;
    }
    try {
      await api.clipart.insert({ path: item.path, x, y, scale: factor });
      set({ error: null, status: strings.clipart.inserted(item.name) });
      await useDocumentStore.getState().refresh();
      return true;
    } catch (error) {
      set({ error: message(error), status: null });
      return false;
    }
  },
}));
