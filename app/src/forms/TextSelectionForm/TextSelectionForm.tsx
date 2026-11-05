import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { GlyphBox, Path, Point, TextArc, TextSpec } from '../../bridge/document';
import { circlePath, rotationAbout, transformPath, translation } from '../../lib/geometry';
import { strings } from '../../lib/formStrings';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  arcHandleAt,
  arcHandles,
  archFromDrag,
  type ArchHandle,
  archHandles,
  clickGap,
  gapAt,
  radiusFromDrag,
  rotateFromDrag,
  type ArcHandle,
} from './model';

import './TextSelectionForm.scss';

const text = strings.text.selection;
const HANDLE_PX = 7;

interface Loaded {
  id: number;
  spec: TextSpec;
  origin: Point;
  glyphs: GlyphBox[];
  height: number;
  box: [number, number, number, number];
  /** The circle arched text follows; null for straight text. */
  arc: TextArc | null;
  /** The text's turn in degrees. */
  rotation: number;
  /** The letters as drawn, for previews of turns and moves. */
  paths: Path[];
}

/** A square handle (turn), a diamond (radius) or a cross (move), `r` millimetres across. */
function handleMark(kind: ArcHandle, at: Point, r: number): Path {
  const line = (a: Point, b: Point) => ({
    start: a,
    closed: false,
    segments: [{ kind: 'line' as const, to: b }],
  });
  if (kind === 'move') {
    return {
      subpaths: [
        line({ x: at.x - r, y: at.y }, { x: at.x + r, y: at.y }),
        line({ x: at.x, y: at.y - r }, { x: at.x, y: at.y + r }),
      ],
    };
  }
  const corners =
    kind === 'rotate'
      ? [
          { x: at.x - r, y: at.y - r },
          { x: at.x + r, y: at.y - r },
          { x: at.x + r, y: at.y + r },
          { x: at.x - r, y: at.y + r },
        ]
      : [
          { x: at.x, y: at.y - r },
          { x: at.x + r, y: at.y },
          { x: at.x, y: at.y + r },
          { x: at.x - r, y: at.y },
        ];
  const [first, ...rest] = corners;
  return {
    subpaths: [
      {
        start: first ?? at,
        closed: true,
        segments: rest.map((to) => ({ kind: 'line' as const, to })),
      },
    ],
  };
}

/** The single selected text with its letters, reloaded after every change. */
function useSelectedText(): Loaded | null {
  const ids = useDocumentStore((store) => store.selection.ids);
  const revision = useDocumentStore((store) => store.revision);
  const entities = useDocumentStore((store) => store.entities);
  const only = ids.length === 1 ? ids[0] : undefined;
  const summary = entities.find((e) => e.id === only && e.kind === 'text');
  const id = summary?.id ?? null;
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  useEffect(() => {
    if (id === null) {
      return;
    }
    let live = true;
    void Promise.all([api.entities.get(id), api.text.glyphs(id)])
      .then(([entity, reply]) => {
        if (live && entity.kind === 'text' && summary !== undefined) {
          setLoaded({
            id,
            spec: entity.spec,
            origin: entity.origin,
            glyphs: reply.glyphs,
            height: reply.height,
            box: summary.bbox,
            arc: reply.arc,
            rotation: reply.rotation ?? 0,
            paths: entity.paths,
          });
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [id, revision, summary]);
  return id === null || loaded?.id !== id ? null : loaded;
}

/** Text Selection: kern letters, space lines and arch a selected text in the view. */
export function TextSelectionForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const updateText = useDocumentStore((store) => store.updateText);
  const current = useSelectedText();
  const currentRef = useRef(current);
  useEffect(() => {
    currentRef.current = current;
  }, [current]);
  const [dragPreview, setDragPreview] = useState<Path[] | null>(null);
  const dragRef = useRef<{ from: Point; handle: ArchHandle | null; arc: ArcHandle | null } | null>(
    null,
  );

  const handles = useMemo<Path[]>(() => {
    // Text set on a curve follows the curve: it has no arch handles.
    if (current === null || current.spec.onCurve !== undefined) {
      return [];
    }
    const pxPerMm = useViewStore.getState().camera.pxPerMm;
    const r = (HANDLE_PX * 0.6) / pxPerMm;
    const { top, bottom } = archHandles(current.box);
    const marks = [circlePath(top.x, top.y, r), circlePath(bottom.x, bottom.y, r)];
    if (current.arc !== null) {
      const extra = arcHandles(current.arc, current.height);
      for (const kind of ['rotate', 'radius', 'move'] as const) {
        for (const at of extra[kind]) {
          marks.push(handleMark(kind, at, r));
        }
      }
    }
    return marks;
  }, [current]);
  usePreview(dragPreview ?? handles);

  const controller = useMemo<DrawController>(() => {
    const onHandle = (at: Point): ArchHandle | null => {
      const loaded = currentRef.current;
      if (loaded === null || loaded.spec.onCurve !== undefined) {
        return null;
      }
      const reach = HANDLE_PX / useViewStore.getState().camera.pxPerMm;
      const { top, bottom } = archHandles(loaded.box);
      const near = (h: Point) => Math.hypot(h.x - at.x, h.y - at.y) <= reach;
      return near(top) ? 'top' : near(bottom) ? 'bottom' : null;
    };
    const arched = (dy: number, handle: ArchHandle) => {
      const loaded = currentRef.current;
      return loaded === null ? null : archFromDrag(loaded.spec, dy, handle);
    };
    const arcHandleUnder = (at: Point): ArcHandle | null => {
      const loaded = currentRef.current;
      if (loaded === null || loaded.arc === null) {
        return null;
      }
      const reach = HANDLE_PX / useViewStore.getState().camera.pxPerMm;
      return arcHandleAt(arcHandles(loaded.arc, loaded.height), at, reach);
    };
    /** Where an arc handle drag leaves the text: a turn, a move or a new radius. */
    const arcDragResult = (kind: ArcHandle, from: Point, to: Point, snap: boolean) => {
      const loaded = currentRef.current;
      if (loaded === null || loaded.arc === null) {
        return null;
      }
      const arc = loaded.arc;
      if (kind === 'rotate') {
        const degrees = rotateFromDrag(arc, from, to, snap);
        return { kind, matrix: rotationAbout(degrees, arc.centre.x, arc.centre.y) } as const;
      }
      if (kind === 'move') {
        return { kind, matrix: translation(to.x - from.x, to.y - from.y) } as const;
      }
      const next = radiusFromDrag(loaded.spec, loaded.origin, loaded.rotation, arc, from, to);
      return next === null ? null : ({ kind, ...next } as const);
    };
    return {
      cursor: 'text',
      pointerDown: (p) => {
        const at = { x: p.rawX, y: p.rawY };
        const arc = arcHandleUnder(at);
        dragRef.current = { from: at, handle: arc === null ? onHandle(at) : null, arc };
      },
      pointerMove: (p) => {
        const drag = dragRef.current;
        const loaded = currentRef.current;
        if (drag !== null && drag.arc !== null && loaded !== null) {
          const result = arcDragResult(drag.arc, drag.from, { x: p.rawX, y: p.rawY }, p.ctrl);
          if (result === null) {
            return;
          }
          if (result.kind !== 'radius') {
            setDragPreview(loaded.paths.map((path) => transformPath(path, result.matrix)));
            return;
          }
          const turn = rotationAbout(loaded.rotation, result.origin.x, result.origin.y);
          void api.text
            .preview({ kind: 'plain', spec: result.spec, x: result.origin.x, y: result.origin.y })
            .then((reply) => {
              if (dragRef.current !== null) {
                setDragPreview(reply.paths.map((path) => transformPath(path, turn)));
              }
            })
            .catch(() => undefined);
          return;
        }
        if (drag === null || drag.handle === null || loaded === null) {
          return;
        }
        const spec = arched(p.rawY - drag.from.y, drag.handle);
        if (spec !== null) {
          void api.text
            .preview({ kind: 'plain', spec, x: loaded.origin.x, y: loaded.origin.y })
            .then((reply) => {
              if (dragRef.current !== null) {
                setDragPreview(reply.paths);
              }
            })
            .catch(() => undefined);
        }
      },
      pointerUp: (p) => {
        const drag = dragRef.current;
        dragRef.current = null;
        setDragPreview(null);
        const loaded = currentRef.current;
        if (drag === null) {
          return;
        }
        const at = { x: p.rawX, y: p.rawY };
        if (drag.arc !== null && loaded !== null) {
          const result = arcDragResult(drag.arc, drag.from, at, p.ctrl);
          if (result === null) {
            return;
          }
          if (result.kind === 'radius') {
            void updateText(loaded.id, result.spec, result.origin.x, result.origin.y);
          } else if (Math.hypot(at.x - drag.from.x, at.y - drag.from.y) > 1e-9) {
            void useDocumentStore.getState().transformSelection(result.matrix, [loaded.id]);
          }
          return;
        }
        if (drag.handle !== null && loaded !== null) {
          const spec = arched(at.y - drag.from.y, drag.handle);
          if (spec !== null) {
            void updateText(loaded.id, spec, loaded.origin.x, loaded.origin.y);
          }
          return;
        }
        const gap = loaded === null ? null : gapAt(loaded.glyphs, at, loaded.height);
        if (loaded !== null && gap !== null) {
          const keys = { shift: p.shift, ctrl: p.ctrl, alt: p.alt === true };
          const spec = clickGap(loaded.spec, loaded.glyphs, gap, keys);
          void updateText(loaded.id, spec, loaded.origin.x, loaded.origin.y);
          return;
        }
        // Anywhere else: pick the text under the pointer.
        const tolerance = HANDLE_PX / useViewStore.getState().camera.pxPerMm;
        void api.entities.hitTest({ x: at.x, y: at.y, tolerance }).then(({ ids }) => {
          const doc = useDocumentStore.getState();
          const hit = ids.find((id) => doc.entities.some((e) => e.id === id && e.kind === 'text'));
          if (hit !== undefined) {
            void doc.select([hit], 'set');
          }
        });
      },
      cancel: () => {
        dragRef.current = null;
        setDragPreview(null);
      },
    };
  }, [updateText]);
  useDrawController(controller);

  return (
    <div className="text-selection">
      <p className="text-selection__hint">{current === null ? text.pick : text.hint}</p>
      <FormButtons onClose={closeForm} onPrimary={closeForm} primaryLabel={strings.form.ok} />
    </div>
  );
}
