// Zoom box: drag a rectangle to fill the view with it; a plain click zooms
// in around the point.

import { useViewStore } from '../../../state/viewStore';
import { rectBetween, zoomAt, type Point2 } from '../camera';
import type { OverlayPainter } from '../renderer';
import { DRAG_THRESHOLD_PX, type PointerInfo, type ToolController, type ToolHost } from './types';

export class ZoomBoxTool implements ToolController {
  readonly id = 'zoomBox' as const;
  readonly cursor = 'zoom-in';
  private host: ToolHost | null = null;
  private from: Point2 | null = null;
  private fromScreen: Point2 | null = null;

  activate(host: ToolHost): void {
    this.host = host;
  }

  deactivate(): void {
    this.cancel();
    this.host = null;
  }

  overlay(): OverlayPainter | null {
    return null;
  }

  pointerDown(p: PointerInfo): void {
    if (p.button !== 0) {
      return;
    }
    this.from = p.job;
    this.fromScreen = p.screen;
  }

  pointerMove(p: PointerInfo): void {
    const host = this.host;
    if (host === null || this.from === null) {
      return;
    }
    host.renderer.update({ rubberBand: { from: this.from, to: p.job, mode: 'inside' } });
    host.requestDraw();
  }

  pointerUp(p: PointerInfo): void {
    const host = this.host;
    const from = this.from;
    const fromScreen = this.fromScreen;
    this.from = null;
    this.fromScreen = null;
    if (host === null || from === null || fromScreen === null) {
      return;
    }
    host.renderer.update({ rubberBand: null });
    const view = useViewStore.getState();
    const dragged =
      Math.hypot(p.screen.x - fromScreen.x, p.screen.y - fromScreen.y) >= DRAG_THRESHOLD_PX;
    if (dragged) {
      view.zoomToBounds(rectBetween(from, p.job));
    } else {
      view.setCamera(zoomAt(view.camera, view.viewport, p.screen, 2));
    }
    host.requestDraw();
  }

  cancel(): void {
    this.from = null;
    this.fromScreen = null;
    this.host?.renderer.update({ rubberBand: null });
    this.host?.requestDraw();
  }
}
