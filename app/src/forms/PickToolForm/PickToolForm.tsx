import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Path, Point } from '../../bridge/document';
import { SCISSORS_CLOSED, SCISSORS_OPEN } from '../../lib/cursors';
import { strings } from '../../lib/formStrings';
import { HoverProbe } from '../../lib/hoverProbe';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController, PreviewOverlay } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { pickTolerance } from '../FilletForm/model';
import { FormButtons } from '../FormButtons/FormButtons';
import { REJOIN_TOLERANCE, bladeTo, rejoinIds, trimsHere } from './model';

import './PickToolForm.scss';

const text = strings.editObjects.trim;

/** How long a group the tool refuses stays highlighted. */
const FLASH_MS = 400;

/**
 * Interactive Vector Trim: a click trims the span under it back to the
 * nearest crossings; holding the button and moving trims every span the
 * pointer passes over. Rejoin Trimmed Sections joins the trimmed pieces
 * that meet when the form closes. A group is refused and flashes.
 */
export function TrimForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const [missed, setMissed] = useState<string | null>(null);
  const [rejoin, setRejoin] = useState(true);
  const [flash, setFlash] = useState<Path[]>([]);
  const rejoinRef = useRef(rejoin);
  useEffect(() => {
    rejoinRef.current = rejoin;
  }, [rejoin]);
  const trimmed = useRef<number[]>([]);
  const pressed = useRef(false);
  const busy = useRef(false);
  const lastTrim = useRef<Point | null>(null);
  // Where the drag's last cut ended, and the pointer it has not cut to yet.
  const bladeFrom = useRef<Point | null>(null);
  const bladeAt = useRef<Point | null>(null);

  // Rejoin Trimmed Sections: when the form closes, the trimmed vectors
  // that are left are joined where their ends meet.
  useEffect(
    () => () => {
      const doc = useDocumentStore.getState();
      const ids = rejoinIds(trimmed.current, new Set(doc.entities.map((e) => e.id)));
      if (rejoinRef.current && ids.length > 0) {
        void doc.joinSelectionOf(ids, REJOIN_TOLERANCE);
      }
    },
    [],
  );

  const controller = useMemo<DrawController>(() => {
    const trimAt = async (at: Point, click: boolean) => {
      const view = useViewStore.getState();
      const tolerance = pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
      if (busy.current || !trimsHere(click ? null : lastTrim.current, at, tolerance)) {
        return;
      }
      busy.current = true;
      lastTrim.current = at;
      try {
        const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance });
        const [id] = ids;
        if (id === undefined) {
          if (click) {
            setMissed(text.missed);
          }
          return;
        }
        const entity = await api.entities.get(id);
        if (entity.kind === 'group') {
          const paths = await Promise.all(entity.children.map((child) => api.entities.get(child)));
          setFlash(paths.flatMap((e) => ('path' in e ? [e.path] : [])));
          window.setTimeout(() => setFlash([]), FLASH_MS);
          setMissed(text.group);
          return;
        }
        try {
          await api.ops.trim(id, at.x, at.y);
          trimmed.current.push(id);
          setMissed(null);
        } catch (error: unknown) {
          if (click) {
            setMissed(error instanceof Error ? error.message : String(error));
          }
        }
        await useDocumentStore.getState().refresh();
      } finally {
        busy.current = false;
      }
    };
    // While the button is held, the stretch the pointer moved along since
    // the last cut is cut, so no span it passed over is missed while a
    // request is in flight or between two pointer events.
    const cutAlong = async () => {
      const view = useViewStore.getState();
      const tolerance = pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
      while (!busy.current && bladeAt.current !== null) {
        const blade = bladeTo(bladeFrom.current, bladeAt.current, tolerance);
        if (blade === null) {
          return;
        }
        busy.current = true;
        bladeFrom.current = blade.to;
        try {
          const reply = await api.drawTools.snip(
            blade.from.x,
            blade.from.y,
            blade.to.x,
            blade.to.y,
          );
          trimmed.current.push(...reply.ids);
          setMissed(null);
          await useDocumentStore.getState().refresh();
        } catch {
          // The stretch crossed no vector.
        } finally {
          busy.current = false;
        }
      }
    };
    // Closed scissors, opening over a span a click would trim.
    const hover = new HoverProbe(async (at) => {
      const view = useViewStore.getState();
      const tolerance = pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
      const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance });
      const [id] = ids;
      if (id === undefined) {
        return SCISSORS_CLOSED;
      }
      const { trims } = await api.ops.trimProbe(id, at.x, at.y);
      return trims ? SCISSORS_OPEN : SCISSORS_CLOSED;
    }, SCISSORS_CLOSED);
    return {
      get cursor() {
        return hover.cursor;
      },
      pointerDown: (p) => {
        pressed.current = true;
        const at = { x: p.rawX, y: p.rawY };
        bladeFrom.current = at;
        bladeAt.current = null;
        void trimAt(at, true).then(() => {
          hover.reset();
          void cutAlong();
        });
      },
      pointerMove: (p) => {
        if (pressed.current) {
          bladeAt.current = { x: p.rawX, y: p.rawY };
          void cutAlong();
          return;
        }
        hover.move({ x: p.rawX, y: p.rawY });
      },
      pointerUp: (p) => {
        if (pressed.current) {
          bladeAt.current = { x: p.rawX, y: p.rawY };
          void cutAlong();
        }
        pressed.current = false;
        lastTrim.current = null;
      },
      cancel: () => {
        pressed.current = false;
        lastTrim.current = null;
        bladeFrom.current = null;
        bladeAt.current = null;
      },
    };
  }, []);
  useDrawController(controller);

  const overlays = useMemo<PreviewOverlay[]>(
    () => flash.map((path) => ({ path, tone: 'accent' })),
    [flash],
  );
  useOverlays(overlays);

  return (
    <div className="pick-tool-form">
      <p className="pick-tool-form__hint">{text.hint}</p>
      {missed === null ? null : <p className="pick-tool-form__notice">{missed}</p>}
      <CheckField checked={rejoin} label={text.rejoin} onChange={setRejoin} />
      <FormButtons onClose={closeForm} />
    </div>
  );
}
