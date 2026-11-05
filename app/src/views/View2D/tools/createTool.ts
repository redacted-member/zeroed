// Creation tool: active while a Create Vectors form is open. Forwards pointer
// and key events to the form's draw controller and asks for snaps so the
// form receives snapped points.

import { useDrawStore, type DrawPointer } from '../../../state/drawStore';
import { useViewStore } from '../../../state/viewStore';
import { clearQuickKeys, quickKeyDown } from './quickInput';
import type { OverlayPainter } from '../renderer';
import type { PointerInfo, ToolController, ToolHost } from './types';

function toDrawPointer(p: PointerInfo): DrawPointer {
  return {
    x: p.snapped.x,
    y: p.snapped.y,
    rawX: p.job.x,
    rawY: p.job.y,
    shift: p.shift,
    ctrl: p.ctrl,
    alt: p.alt,
  };
}

export class CreateTool implements ToolController {
  readonly id = 'create' as const;
  readonly cursor = 'crosshair';
  private host: ToolHost | null = null;

  activate(host: ToolHost): void {
    this.host = host;
    host.renderer.update({ handleMode: 'none', nodeOverlay: null, hoverId: null });
  }

  deactivate(): void {
    clearQuickKeys();
    useDrawStore.getState().controller?.cancel();
    this.host?.clearSnap();
    this.host = null;
  }

  overlay(): OverlayPainter | null {
    return null;
  }

  pointerDown(p: PointerInfo): void {
    if (p.button !== 0) {
      return;
    }
    useDrawStore.getState().controller?.pointerDown(toDrawPointer(p));
    this.host?.requestDraw();
  }

  pointerMove(p: PointerInfo): void {
    const host = this.host;
    if (host === null) {
      return;
    }
    host.probe(p.job);
    const controller = useDrawStore.getState().controller;
    if (controller !== null) {
      host.setCursor(controller.cursor ?? this.cursor);
      controller.pointerMove(toDrawPointer(p));
    }
    host.requestDraw();
  }

  pointerUp(p: PointerInfo): void {
    useDrawStore.getState().controller?.pointerUp(toDrawPointer(p));
    this.host?.requestDraw();
  }

  doubleClick(p: PointerInfo): void {
    useDrawStore.getState().controller?.doubleClick?.(toDrawPointer(p));
    this.host?.requestDraw();
  }

  keyDown(event: KeyboardEvent): boolean {
    const controller = useDrawStore.getState().controller;
    const quick = controller?.quickEntry;
    const typed = quickKeyDown(event, quick !== undefined, (entry) => {
      const cursor = useViewStore.getState().cursor ?? { x: 0, y: 0 };
      if (controller?.quickEntry?.(entry, cursor) === true) {
        this.host?.requestDraw();
      }
    });
    if (typed) {
      return true;
    }
    const handled = controller?.keyDown?.(event) ?? false;
    if (handled) {
      this.host?.requestDraw();
    }
    return handled;
  }

  cancel(): void {
    clearQuickKeys();
    useDrawStore.getState().controller?.cancel();
    this.host?.clearSnap();
    this.host?.requestDraw();
  }
}
