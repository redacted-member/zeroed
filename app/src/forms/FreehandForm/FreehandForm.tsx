import { useEffect, useId, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Path, Point } from '../../bridge/document';
import { fadeBitmaps } from '../../lib/bitmapCommands';
import { extendTrail, ringPoints, stabilise, stabiliserPx } from '../../lib/freehand';
import { openEndNear } from '../../lib/spans';
import { strings } from '../../lib/formStrings';
import { useDrawController } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import { useDrawStore, type DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { fadeOf, opacityOf, selectedBitmap } from '../TraceBitmapForm/model';
import {
  INITIAL_FREEHAND,
  freehandRequest,
  smoothnessValue,
  stabilisationValue,
  trailPreview,
  type FreehandFormState,
} from './model';

import './FreehandForm.scss';

const text = strings.drawTools.freehand;
const END_PX = 8;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The open vector end within `px` screen pixels of `at`, if any. */
async function openEndAt(at: Point, pxPerMm: number): Promise<{ id: number; point: Point } | null> {
  const tolerance = END_PX / pxPerMm;
  const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance });
  for (const id of ids) {
    const entity = await api.entities.get(id);
    if (entity.kind === 'path') {
      const end = openEndNear(entity.path, at, tolerance);
      if (end !== null) {
        return { id, point: end.point };
      }
    }
  }
  return null;
}

interface Stroke {
  trail: Point[];
  brush: Point;
  cursor: Point;
  /** The open vector this stroke carries on, when it started on one's end. */
  from: number | null;
}

/** Freehand Drawing: each drag sketches a stroke that becomes a smooth vector. */
export function FreehandForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState<FreehandFormState>(INITIAL_FREEHAND);
  const [notice, setNotice] = useState<string | null>(null);
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);
  const strokeRef = useRef<Stroke | null>(null);
  const smoothId = useId();
  const stabiliseId = useId();
  const fadeId = useId();

  const bitmap = useDocumentStore((store) => selectedBitmap(store.entities, store.selection.ids));
  const [fade, setFade] = useState(0);
  useEffect(() => {
    if (bitmap === null) {
      return;
    }
    let live = true;
    void api.entities
      .get(bitmap)
      .then((entity) => {
        if (live && entity.kind === 'bitmap') {
          setFade(fadeOf(entity.opacity));
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [bitmap]);
  const applyFade = () => {
    if (bitmap !== null) {
      void fadeBitmaps([bitmap], opacityOf(fade));
    }
  };

  const controller = useMemo<DrawController>(() => {
    const radiusMm = () => {
      const pxPerMm = useViewStore.getState().camera.pxPerMm;
      return pxPerMm > 0 ? stabiliserPx(stabilisationValue(stateRef.current)) / pxPerMm : 0;
    };
    const show = (stroke: Stroke | null) => {
      strokeRef.current = stroke;
      if (stroke === null) {
        useDrawStore.getState().setPreview([]);
        return;
      }
      const paths: Path[] = [...trailPreview(stroke.trail)];
      const radius = radiusMm();
      if (radius > 0) {
        // The dead zone round the brush.
        const [start, ...ring] = ringPoints(stroke.brush, radius);
        if (start !== undefined) {
          paths.push({
            subpaths: [
              { start, closed: true, segments: ring.map((to) => ({ kind: 'line' as const, to })) },
            ],
          });
        }
      }
      useDrawStore.getState().setPreview(paths);
    };
    const finish = async (close: boolean, joinTo: Point | null) => {
      const stroke = strokeRef.current;
      if (stroke === null) {
        return;
      }
      show(null);
      const view = useViewStore.getState();
      const pxPerMm = view.camera.pxPerMm;
      let trail = stroke.trail;
      if (stateRef.current.endAtCursor) {
        trail = extendTrail(trail, stroke.cursor, pxPerMm);
      }
      // Ctrl on release over another vector's end joins the stroke to it.
      const target = joinTo === null ? null : await openEndAt(joinTo, pxPerMm);
      if (target !== null) {
        trail = [...trail, target.point];
      }
      const request = freehandRequest(
        trail,
        stateRef.current,
        pxPerMm,
        view.snapRadiusPx,
        close,
        stroke.cursor,
      );
      if (request === null) {
        setNotice(text.tooShort);
        return;
      }
      try {
        const reply = await api.drawTools.freehand(request);
        const join = [stroke.from, target?.id ?? null].filter((id): id is number => id !== null);
        if (join.length > 0 && !request.closed) {
          await api.ops.join([...join, reply.id], END_PX / pxPerMm);
        }
      } catch (caught) {
        setNotice(message(caught));
      }
      await useDocumentStore.getState().refresh();
    };
    return {
      cursor: 'crosshair',
      pointerDown: (p) => {
        setNotice(null);
        const at = { x: p.x, y: p.y };
        show({ trail: [at], brush: at, cursor: at, from: null });
        if (p.ctrl) {
          // Ctrl on the end of an open vector carries it on.
          const pxPerMm = useViewStore.getState().camera.pxPerMm;
          void openEndAt({ x: p.rawX, y: p.rawY }, pxPerMm)
            .then((end) => {
              const stroke = strokeRef.current;
              if (end !== null && stroke !== null) {
                show({ ...stroke, trail: [end.point, ...stroke.trail.slice(1)], from: end.id });
              }
            })
            .catch(() => undefined);
        }
      },
      pointerMove: (p) => {
        const stroke = strokeRef.current;
        if (stroke === null) {
          return;
        }
        const cursor = { x: p.rawX, y: p.rawY };
        const brush = stabilise(stroke.brush, cursor, radiusMm());
        const pxPerMm = useViewStore.getState().camera.pxPerMm;
        show({ ...stroke, cursor, brush, trail: extendTrail(stroke.trail, brush, pxPerMm) });
      },
      pointerUp: (p) => {
        void finish(false, p.ctrl ? { x: p.rawX, y: p.rawY } : null);
      },
      keyDown: (event) => {
        if (event.key === 'Tab' && strokeRef.current !== null) {
          void finish(true, null);
          return true;
        }
        return false;
      },
      cancel: () => show(null),
    };
  }, []);
  useDrawController(controller);

  const slider = (
    id: string,
    label: string,
    value: string,
    onChange: (value: string) => void,
  ): JSX.Element => (
    <>
      <label className="freehand-form__note" htmlFor={id}>
        {label}
      </label>
      <input
        className="freehand-form__range"
        id={id}
        max={10}
        min={0}
        onChange={(event) => onChange(event.target.value)}
        step={1}
        type="range"
        value={value}
      />
    </>
  );

  return (
    <div className="freehand-form">
      <p className="freehand-form__hint">{text.hint}</p>
      {notice === null ? null : (
        <p className="freehand-form__notice" role="status">
          {notice}
        </p>
      )}
      <fieldset className="freehand-form__group">
        <legend className="freehand-form__legend">{text.options}</legend>
        {slider(
          smoothId,
          text.smoothnessLabel(smoothnessValue(state)),
          state.smoothness,
          (smoothness) => setState((s) => ({ ...s, smoothness })),
        )}
        <p className="freehand-form__note">{text.smoothnessHint}</p>
        {slider(
          stabiliseId,
          text.stabilisationLabel(stabilisationValue(state)),
          state.stabilisation,
          (stabilisation) => setState((s) => ({ ...s, stabilisation })),
        )}
        <p className="freehand-form__note">{text.stabilisationHint}</p>
        <CheckField
          checked={state.endAtCursor}
          label={text.endAtCursor}
          onChange={(endAtCursor) => setState((s) => ({ ...s, endAtCursor }))}
        />
      </fieldset>
      {bitmap !== null ? (
        <fieldset className="freehand-form__group">
          <legend className="freehand-form__legend">{text.fade}</legend>
          <label className="freehand-form__note" htmlFor={fadeId}>
            {strings.createVectors.traceBitmap.fadingLabel(fade)}
          </label>
          <input
            className="freehand-form__range"
            id={fadeId}
            max={100}
            min={0}
            onChange={(event) => setFade(Number(event.target.value))}
            onKeyUp={applyFade}
            onPointerUp={applyFade}
            step={5}
            type="range"
            value={fade}
          />
        </fieldset>
      ) : null}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
