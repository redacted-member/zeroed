// The triangulated model the Export Model form calculated, shown in the 3D
// view in place of the relief until the form closes or a setting changes.

import { create } from 'zustand';

import type { BackFace } from '../bridge/exports';

/** A calculated export mesh and how it was closed. */
export interface ShownMesh {
  /** Nine floats a triangle, job mm. */
  xyz: Float32Array;
  back: BackFace;
}

interface MeshPreviewState {
  mesh: ShownMesh | null;
  /** Draw the triangle edges rather than shaded faces. */
  wireframe: boolean;
  show: (mesh: ShownMesh) => void;
  clear: () => void;
  setWireframe: (wireframe: boolean) => void;
}

export const useMeshPreviewStore = create<MeshPreviewState>()((set) => ({
  mesh: null,
  wireframe: false,
  show: (mesh) => set({ mesh }),
  clear: () => set({ mesh: null }),
  setWireframe: (wireframe) => set({ wireframe }),
}));
