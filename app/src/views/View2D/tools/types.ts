// Contract between the interaction controller and the tools it drives.

import type { ToolId } from '../../../state/viewStore';
import type { Point2 } from '../camera';
import type { OverlayPainter, View2DRenderer } from '../renderer';

export interface PointerInfo {
  /** Canvas pixels. */
  screen: Point2;
  /** Job millimetres under the pointer. */
  job: Point2;
  /** Job millimetres after snapping; equals `job` when nothing snapped. */
  snapped: Point2;
  button: number;
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
}

export interface ToolHost {
  readonly renderer: View2DRenderer;
  requestDraw(): void;
  /** Asks for snap candidates around a job point; the result lands in the view store. */
  probe(job: Point2, exclude?: readonly number[]): void;
  clearSnap(): void;
  setCursor(cursor: string): void;
  /** Millimetres per screen pixel at the current zoom. */
  mmPerPx(): number;
  /** Switches the active tool. */
  switchTool(tool: ToolId): void;
}

export interface ToolController {
  readonly id: ToolId;
  readonly cursor: string;
  activate(host: ToolHost): void | Promise<void>;
  deactivate(): void;
  pointerDown(p: PointerInfo): void | Promise<void>;
  pointerMove(p: PointerInfo): void;
  pointerUp(p: PointerInfo): void | Promise<void>;
  doubleClick?(p: PointerInfo): void | Promise<void>;
  /** Returns true when the key was consumed. */
  keyDown?(event: KeyboardEvent, pointer: Point2 | null): boolean | Promise<boolean>;
  cancel(): void;
  overlay(): OverlayPainter | null;
}

export const DRAG_THRESHOLD_PX = 3;
export const HANDLE_HIT_PX = 6;
export const NODE_HIT_PX = 6;
export const PICK_TOLERANCE_PX = 6;
