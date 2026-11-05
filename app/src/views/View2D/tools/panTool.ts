// Pan tool: left drag moves the view. Middle drag and space drag pan in
// every tool; this one exists for the toolbar button.

import { useViewStore } from '../../../state/viewStore';
import { panBy, type Point2 } from '../camera';
import type { OverlayPainter } from '../renderer';
import type { PointerInfo, ToolController, ToolHost } from './types';

export class PanTool implements ToolController {
  readonly id = 'pan' as const;
  readonly cursor = 'grab';
  private host: ToolHost | null = null;
  private last: Point2 | null = null;

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
    this.last = p.screen;
    this.host?.setCursor('grabbing');
  }

  pointerMove(p: PointerInfo): void {
    if (this.last === null) {
      return;
    }
    const view = useViewStore.getState();
    view.setCamera(panBy(view.camera, p.screen.x - this.last.x, p.screen.y - this.last.y));
    this.last = p.screen;
  }

  pointerUp(): void {
    this.last = null;
    this.host?.setCursor(this.cursor);
  }

  cancel(): void {
    this.last = null;
    this.host?.setCursor(this.cursor);
  }
}
