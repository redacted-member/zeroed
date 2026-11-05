// Development only: exposes the stores and the kernel client on the window so
// that browser automation and manual debugging can read live state. Never
// installed in production builds.

import { api } from '../bridge/formApi';
import type * as AssemblyFitStoreModule from '../state/assemblyFitStore';
import type * as AssemblyStoreModule from '../state/assemblyStore';
import type * as FurnitureStoreModule from '../state/furnitureStore';
import type * as PreviewStoreModule from '../state/previewStore';
import { useDocumentStore } from '../state/documentStore';
import { useDrawStore } from '../state/drawStore';
import { useModelStore } from '../state/modelStore';
import { usePrefsStore } from '../state/prefsStore';
import { useUiStore } from '../state/uiStore';
import { useViewStore } from '../state/viewStore';

export interface DevTools {
  api: typeof api;
  /** The assembly store module, loaded on demand like the Assembly tab. */
  assembly: () => Promise<typeof AssemblyStoreModule>;
  /** The Assembly View's fit and explode store module, loaded on demand. */
  assemblyFit: () => Promise<typeof AssemblyFitStoreModule>;
  /** The furniture store module, loaded on demand like the Furniture Designer. */
  furniture: () => Promise<typeof FurnitureStoreModule>;
  /** The toolpath preview store module, loaded on demand. */
  preview: () => Promise<typeof PreviewStoreModule>;
  /** The 3D view's renderer while the view is up, for reading its state. */
  view3d: unknown;
  useDocumentStore: typeof useDocumentStore;
  useDrawStore: typeof useDrawStore;
  useModelStore: typeof useModelStore;
  usePrefsStore: typeof usePrefsStore;
  useUiStore: typeof useUiStore;
  useViewStore: typeof useViewStore;
}

declare global {
  interface Window {
    carveDev?: DevTools;
  }
}

export function installDevTools(): void {
  if (!import.meta.env.DEV) {
    return;
  }
  window.carveDev = {
    api,
    assembly: () => import('../state/assemblyStore'),
    assemblyFit: () => import('../state/assemblyFitStore'),
    furniture: () => import('../state/furnitureStore'),
    preview: () => import('../state/previewStore'),
    view3d: null,
    useDocumentStore,
    useDrawStore,
    useModelStore,
    usePrefsStore,
    useUiStore,
    useViewStore,
  };
}
