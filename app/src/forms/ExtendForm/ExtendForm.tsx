import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Path, Point } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { openEndNear } from '../../lib/spans';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController, PreviewOverlay } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { pickTolerance } from '../FilletForm/model';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  along,
  endRay,
  linePath,
  markPath,
  nearestPoint,
  raysMeet,
  rayHits,
  sameEnd,
  type OpenEndRef,
  type Ray,
} from './model';

import './ExtendForm.scss';

const text = strings.editObjects.extend;

interface Target {
  end: OpenEndRef;
  ray: Ray;
}

/** What the pointer is over: an end to pick, or the point to extend to. */
type Hover =
  | { kind: 'none' }
  | { kind: 'end'; end: OpenEndRef; ray: Ray }
  | { kind: 'point'; at: Point; other: { end: OpenEndRef; ray: Ray } | null };

const NONE: Hover = { kind: 'none' };

/** How far the dashed extension reaches: past the far side of the view. */
function reach(): number {
  const view = useViewStore.getState();
  const { width, height } = view.viewport;
  return (2 * Math.hypot(width, height)) / Math.max(view.camera.pxPerMm, 1e-9);
}

/**
 * Extend Vectors: click the end of an open vector, then click where its
 * straight extension meets a span or another end's extension. A right
 * click starts again with a new end.
 */
export function ExtendForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const revision = useDocumentStore((store) => store.revision);
  const [target, setTarget] = useState<Target | null>(null);
  const [hover, setHover] = useState<Hover>(NONE);
  const [notice, setNotice] = useState<string | null>(null);
  const targetRef = useRef<Target | null>(null);
  const hoverRef = useRef<Hover>(NONE);
  const paths = useRef(new Map<number, Path | null>());
  const seq = useRef(0);

  useEffect(() => {
    paths.current.clear();
  }, [revision]);

  const controller = useMemo<DrawController>(() => {
    const pathOf = async (id: number): Promise<Path | null> => {
      const known = paths.current.get(id);
      if (known !== undefined) {
        return known;
      }
      const entity = await api.entities.get(id);
      const path = entity.kind === 'path' ? entity.path : null;
      paths.current.set(id, path);
      return path;
    };
    const show = (next: Hover) => {
      hoverRef.current = next;
      setHover(next);
    };
    const reset = () => {
      seq.current += 1;
      targetRef.current = null;
      setTarget(null);
      show(NONE);
    };
    const probe = async (at: Point): Promise<Hover> => {
      const view = useViewStore.getState();
      const tolerance = pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
      const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance });
      const found: Array<{ id: number; path: Path }> = [];
      for (const id of ids) {
        const path = await pathOf(id);
        if (path !== null) {
          found.push({ id, path });
        }
      }
      const current = targetRef.current;
      let end: { end: OpenEndRef; ray: Ray } | null = null;
      for (const { id, path } of found) {
        const near = openEndNear(path, at, tolerance);
        const ray = near === null ? null : endRay(path, near.subpath, near.atEnd);
        if (near !== null && ray !== null) {
          const ref = { id, subpath: near.subpath, atEnd: near.atEnd };
          if (current === null || !sameEnd(ref, current.end)) {
            end = { end: ref, ray };
            break;
          }
        }
      }
      if (current === null) {
        return end === null ? NONE : { kind: 'end', ...end };
      }
      if (end !== null) {
        const meet = raysMeet(current.ray, end.ray);
        if (meet !== null) {
          return { kind: 'point', at: meet, other: end };
        }
      }
      const hits = found.flatMap(({ path }) => rayHits(current.ray, reach(), path));
      const point = nearestPoint(hits, at);
      if (point !== null && Math.hypot(point.x - at.x, point.y - at.y) <= tolerance * 2) {
        return { kind: 'point', at: point, other: null };
      }
      return NONE;
    };
    return {
      cursor: 'crosshair',
      pointerDown: (p) => {
        const hovered = hoverRef.current;
        if (hovered.kind === 'end' && targetRef.current === null) {
          const next = { end: hovered.end, ray: hovered.ray };
          targetRef.current = next;
          setTarget(next);
          setNotice(null);
          show(NONE);
          return;
        }
        const current = targetRef.current;
        if (hovered.kind === 'point' && current !== null) {
          const ends = hovered.other === null ? [current.end] : [current.end, hovered.other.end];
          void api.ops
            .extendTo(ends, hovered.at.x, hovered.at.y)
            .then(() => setNotice(null))
            .catch((error: unknown) =>
              setNotice(error instanceof Error ? error.message : String(error)),
            )
            .finally(() => {
              void useDocumentStore.getState().refresh();
            });
          reset();
          return;
        }
        setNotice(current === null ? text.missed : text.noMeet);
        void p;
      },
      pointerMove: (p) => {
        seq.current += 1;
        const mine = seq.current;
        void probe({ x: p.rawX, y: p.rawY })
          .then((next) => {
            if (mine === seq.current) {
              show(next);
            }
          })
          .catch(() => undefined);
      },
      pointerUp: () => undefined,
      rightClick: () => {
        reset();
        setNotice(null);
        return true;
      },
      cancel: reset,
    };
  }, []);
  useDrawController(controller);

  const overlays = useMemo<PreviewOverlay[]>(() => {
    const out: PreviewOverlay[] = [];
    const size = 8 / Math.max(useViewStore.getState().camera.pxPerMm, 1e-9);
    if (target !== null) {
      out.push({ path: linePath(target.ray.origin, along(target.ray, reach())), tone: 'accent' });
    }
    if (hover.kind === 'end') {
      out.push({
        path: linePath(hover.ray.origin, along(hover.ray, reach())),
        tone: 'accent',
        dashed: true,
      });
    } else if (hover.kind === 'point') {
      if (target !== null) {
        out.push({ path: linePath(target.ray.origin, hover.at), tone: 'second', dashed: true });
      }
      if (hover.other !== null) {
        out.push({
          path: linePath(hover.other.ray.origin, hover.at),
          tone: 'second',
          dashed: true,
        });
      }
      out.push({ path: markPath(hover.at, size), tone: 'first' });
    }
    return out;
  }, [target, hover]);
  useOverlays(overlays);

  return (
    <div className="extend-form">
      <p className="extend-form__hint">{target === null ? text.hint : text.pickPoint}</p>
      {notice === null ? null : <p className="extend-form__notice">{notice}</p>}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
