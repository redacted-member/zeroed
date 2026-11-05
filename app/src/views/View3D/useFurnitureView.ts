// The Furniture Designer in the 3D view, while the Assembly View shows:
// the fast preview's panels, the overall width, depth and height lines
// with their lengths in the job unit, and seam picking. Hovering a seam
// lights it and names its joint; a click opens the seam popover there.

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MutableRefObject,
  type RefObject,
} from 'react';

import type { ConnectionRow, Vec3 } from '../../bridge/furniture';
import { formatLength, unitSuffix } from '../../lib/units';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { useFurnitureStore } from '../../state/furnitureStore';
import {
  dimensionGap,
  dimensionLines,
  midpoint,
  nearestSeam,
  placedBox,
  placedSeam,
  previewShown,
  sizeArrows,
  type DimensionAxis,
} from './furnitureSeams';
import type { View3DRenderer } from './renderer';

/** A dimension's length as the view labels it, where it shows (CSS px). */
export interface DimensionLabel {
  axis: DimensionAxis;
  text: string;
  x: number;
  y: number;
}

/** A drag arrow on an outer face, on screen (CSS px), and the parameter it drags. */
export interface ShownArrow {
  axis: DimensionAxis;
  /** The template parameter: width, depth or height. */
  key: string;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  /** The arrow's length in the design, mm. */
  length: number;
}

/** The joint name next to a hovered seam (CSS px). */
export interface SeamLabel {
  key: string;
  x: number;
  y: number;
}

export interface FurnitureViewControl {
  /** Whether a design is open in the designer. */
  active: boolean;
  labels: DimensionLabel[];
  seamLabel: SeamLabel | null;
  arrows: ShownArrow[];
  /** The pointer moved (canvas CSS px and normalised device coordinates); true when on a seam. */
  hoverAt: (x: number | null, y?: number, ndcX?: number, ndcY?: number) => boolean;
  /** A click; true when it opened the seam popover. */
  clickAt: (x: number, y: number, ndcX: number, ndcY: number) => boolean;
}

const NO_SHIFT: Vec3 = [0, 0, 0];

/** The middle of a seam where the Assembly View shows it. */
function placedMiddle(seam: { from: Vec3; to: Vec3 }, origin: Vec3): Vec3 {
  const placed = placedSeam(seam, origin);
  return midpoint(placed.from, placed.to);
}

/** The seam bar's radius: thin, but thick enough to see on a big design, mm. */
function seamRadius(span: number): number {
  return Math.min(6, Math.max(1.5, span / 400));
}

export function useFurnitureView(
  rendererRef: RefObject<View3DRenderer | null>,
  layoutRef: MutableRefObject<(() => void) | null>,
  scheduleDraw: () => void,
): FurnitureViewControl {
  const assembly = useAssemblyStore((s) => s.active);
  const session = useFurnitureStore((s) => s.session);
  const preview = useFurnitureStore((s) => s.preview);
  const connections = useFurnitureStore((s) => s.connections);
  const hover = useFurnitureStore((s) => s.hover);
  const popover = useFurnitureStore((s) => s.popover);
  const units = useDocumentStore((s) => s.job.units);
  const active = assembly && session !== null;
  const framedRef = useRef<string | null>(null);
  const [labels, setLabels] = useState<DimensionLabel[]>([]);
  const [seamLabel, setSeamLabel] = useState<SeamLabel | null>(null);
  const [arrows, setArrows] = useState<ShownArrow[]>([]);

  const origin: Vec3 = preview?.origin ?? NO_SHIFT;
  const box =
    preview === null
      ? null
      : placedBox(
          preview.panels.map((p) => p.box),
          origin,
        );
  // The preview shows while designing; a made design that has settled
  // shows as its pieces, the parts its toolpaths cut.
  const shown = useFurnitureStore(previewShown);
  const showPanels = active && preview !== null && shown;
  const span = box === null ? 0 : Math.max(...box.max.map((v, i) => v - (box.min[i] ?? 0)));

  // The preview's panels and dimension lines; the view frames them once per design.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    if (!active || preview === null || box === null) {
      renderer.setFurniturePreview(null, null);
      renderer.setFurnitureDimensions([]);
      if (!active) {
        framedRef.current = null;
      }
      scheduleDraw();
      return;
    }
    renderer.setFurniturePreview(showPanels ? preview.panels : null, box, origin);
    renderer.setFurnitureDimensions(dimensionLines(box, dimensionGap(box)));
    const id = `${session?.template.id ?? ''}:${session?.design ?? 'new'}`;
    if (framedRef.current !== id) {
      framedRef.current = id;
      renderer.setView(renderer.fitted(renderer.view()));
    }
    scheduleDraw();
    // `box` follows `preview`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    active,
    preview,
    showPanels,
    rendererRef,
    scheduleDraw,
    session?.design,
    session?.template.id,
  ]);

  // The seam under the pointer, or the one the popover is open on.
  const lit = popover ?? hover;
  const litSeam = lit === null ? null : (connections.find((c) => c.key === lit.key) ?? null);
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    renderer.setFurnitureSeam(
      active && litSeam !== null ? placedSeam(litSeam.seam, origin) : null,
      seamRadius(span),
      popover !== null,
    );
    scheduleDraw();
  }, [active, litSeam, origin, popover, rendererRef, scheduleDraw, span]);

  // Labels follow the camera: placed after each frame.
  const layout = useCallback(() => {
    const renderer = rendererRef.current;
    const state = useFurnitureStore.getState();
    const shownBox =
      state.preview === null
        ? null
        : placedBox(
            state.preview.panels.map((p) => p.box),
            state.preview.origin,
          );
    if (
      renderer === null ||
      !useAssemblyStore.getState().active ||
      state.session === null ||
      shownBox === null
    ) {
      setLabels((old) => (old.length === 0 ? old : []));
      setSeamLabel((old) => (old === null ? old : null));
      setArrows((old) => (old.length === 0 ? old : []));
      return;
    }
    // Arrows for the template's own width, depth and height lengths.
    const lengths = new Set(
      state.session.template.params.filter((p) => p.kind === 'length').map((p) => p.key),
    );
    const reach = dimensionGap(shownBox) * 1.5;
    const shownArrows: ShownArrow[] = [];
    for (const arrow of sizeArrows(shownBox, reach)) {
      const a = renderer.projectJob(arrow.base);
      const b = renderer.projectJob(arrow.tip);
      if (lengths.has(arrow.axis) && a !== null && b !== null) {
        shownArrows.push({
          axis: arrow.axis,
          key: arrow.axis,
          x1: a.x,
          y1: a.y,
          x2: b.x,
          y2: b.y,
          length: reach,
        });
      }
    }
    setArrows((old) => (JSON.stringify(old) === JSON.stringify(shownArrows) ? old : shownArrows));
    const job = useDocumentStore.getState().job.units;
    const next: DimensionLabel[] = [];
    for (const line of dimensionLines(shownBox, dimensionGap(shownBox))) {
      const at = renderer.projectJob(midpoint(line.from, line.to));
      if (at !== null) {
        next.push({
          axis: line.axis,
          text: `${formatLength(line.length, job)} ${unitSuffix(job)}`,
          x: at.x,
          y: at.y,
        });
      }
    }
    setLabels((old) => (JSON.stringify(old) === JSON.stringify(next) ? old : next));
    const hovered = state.popover === null ? state.hover : null;
    const seam = hovered === null ? null : state.connections.find((c) => c.key === hovered.key);
    const where =
      seam === undefined || seam === null
        ? null
        : renderer.projectJob(placedMiddle(seam.seam, state.preview?.origin ?? [0, 0, 0]));
    const label =
      seam === undefined || seam === null || where === null
        ? null
        : { key: seam.key, x: where.x, y: where.y };
    setSeamLabel((old) => (JSON.stringify(old) === JSON.stringify(label) ? old : label));
  }, [rendererRef]);

  useEffect(() => {
    layoutRef.current = layout;
    return () => {
      layoutRef.current = null;
    };
  }, [layout, layoutRef]);

  // Units changed: the labels say the new unit at once.
  useEffect(() => {
    layout();
  }, [layout, units, preview, hover, popover]);

  const seamAt = useCallback(
    (ndcX: number, ndcY: number): ConnectionRow | null => {
      const renderer = rendererRef.current;
      if (renderer === null) {
        return null;
      }
      const { origin, direction } = renderer.jobRay(ndcX, ndcY);
      const list = useFurnitureStore.getState().connections;
      // Seams are in furniture space: move the ray there.
      const at = useFurnitureStore.getState().preview?.origin ?? [0, 0, 0];
      const start: Vec3 = [origin[0] - at[0], origin[1] - at[1], origin[2] - at[2]];
      return nearestSeam(start, direction as Vec3, list);
    },
    [rendererRef],
  );

  const hoverAt = useCallback(
    (x: number | null, y?: number, ndcX?: number, ndcY?: number): boolean => {
      const store = useFurnitureStore.getState();
      if (!active || x === null || y === undefined || ndcX === undefined || ndcY === undefined) {
        store.setHover(null);
        return false;
      }
      const seam = seamAt(ndcX, ndcY);
      store.setHover(seam === null ? null : { key: seam.key, x, y });
      return seam !== null;
    },
    [active, seamAt],
  );

  const clickAt = useCallback(
    (x: number, y: number, ndcX: number, ndcY: number): boolean => {
      if (!active) {
        return false;
      }
      const seam = seamAt(ndcX, ndcY);
      const store = useFurnitureStore.getState();
      if (seam === null) {
        if (store.popover !== null) {
          store.setPopover(null);
          return true;
        }
        return false;
      }
      store.setPopover({ key: seam.key, x, y });
      return true;
    },
    [active, seamAt],
  );

  return { active, labels, seamLabel, arrows, hoverAt, clickAt };
}
