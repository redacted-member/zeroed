// Hooks that connect a creation form to the drawing view for as long as the
// form is open: the view runs the create tool, forwards events to the
// controller and draws the preview.

import { useEffect } from 'react';

import type { Path } from '../bridge/document';
import {
  useDrawStore,
  type DrawController,
  type PreviewLabel,
  type PreviewOverlay,
} from '../state/drawStore';
import { useUiStore } from '../state/uiStore';
import { useViewStore } from '../state/viewStore';

/**
 * Brings the 2D view up for a form that draws or picks in it: the 3D view
 * alone gives way to the 2D view (the 2D view shows when a
 * drawing tool is picked); a split layout already shows it.
 */
export function showDrawingView(): void {
  const ui = useUiStore.getState();
  if (ui.viewMode === '3d') {
    ui.setViewMode('2d');
  }
}

/** Registers the controller and switches the view to the create tool. */
export function useDrawController(controller: DrawController): void {
  useEffect(() => {
    showDrawingView();
    const view = useViewStore.getState();
    const previousTool = view.activeTool;
    view.setActiveTool('create');
    useDrawStore.getState().setController(controller);
    return () => {
      const draw = useDrawStore.getState();
      if (draw.controller === controller) {
        draw.setController(null);
        draw.setPreview([]);
        draw.setOverlays([]);
      }
      const current = useViewStore.getState();
      if (current.activeTool === 'create') {
        current.setActiveTool(previousTool === 'create' ? 'select' : previousTool);
      }
    };
  }, [controller]);
}

/** Publishes the preview paths the form wants drawn, and takes them down
 * when the form closes (forms without a controller rely on this). */
export function usePreview(paths: Path[]): void {
  useEffect(() => {
    useDrawStore.getState().setPreview(paths);
    return () => {
      const draw = useDrawStore.getState();
      if (draw.preview === paths) {
        draw.setPreview([]);
      }
    };
  }, [paths]);
}

/** Publishes coloured overlay paths while the form is open. */
export function useOverlays(overlays: PreviewOverlay[]): void {
  useEffect(() => {
    useDrawStore.getState().setOverlays(overlays);
    return () => {
      const draw = useDrawStore.getState();
      if (draw.overlays === overlays) {
        draw.setOverlays([]);
      }
    };
  }, [overlays]);
}

/** Shows `labels` in the drawing view while the calling form is open. */
export function useLabels(labels: PreviewLabel[]): void {
  useEffect(() => {
    useDrawStore.getState().setLabels(labels);
    return () => {
      const draw = useDrawStore.getState();
      if (draw.labels === labels) {
        draw.setLabels([]);
      }
    };
  }, [labels]);
}
