import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Point } from '../../bridge/document';
import type { MeasureHit } from '../../bridge/drawTools';
import { isDrag } from '../../lib/measure';
import { strings } from '../../lib/formStrings';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import {
  useDrawStore,
  type DrawController,
  type DrawPointer,
  type PreviewOverlay,
} from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { pickTolerance } from '../FilletForm/model';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  contourRows,
  EMPTY_MEASURE,
  measureMove,
  measurePress,
  measureRelease,
  pointRows,
  rulerPreview,
  spanPath,
  spanRows,
  type MeasureMode,
  type MeasureState,
  type ReadoutRow,
} from './model';

import './MeasureForm.scss';

const text = strings.drawTools.measure;

const MODES: ReadonlyArray<Choice<MeasureMode>> = [
  { value: 'points', label: text.modePoints },
  { value: 'span', label: text.modeSpan },
  { value: 'contour', label: text.modeContour },
];

function Readout({ rows }: { rows: ReadoutRow[] }): JSX.Element {
  return (
    <dl className="measure-form__readout">
      {rows.map((row) => (
        <div className="measure-form__row" key={row.label}>
          <dt className="measure-form__label">{row.label}</dt>
          <dd className="measure-form__value">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Measure - Inspect: Measure Between 2 Points (snapped, with the first
 * point and the cursor position), a Span of a vector, or a vector's
 * Contour Properties. */
export function MeasureForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const units = useDocumentStore((store) => store.job.units);
  const [mode, setMode] = useState<MeasureMode>('points');
  const [measure, setMeasure] = useState<MeasureState>(EMPTY_MEASURE);
  const [cursor, setCursor] = useState<Point | null>(null);
  // undefined: nothing picked yet; null: the click missed every vector.
  const [hit, setHit] = useState<MeasureHit | null | undefined>(undefined);
  const measureRef = useRef<MeasureState>(EMPTY_MEASURE);
  const pressRef = useRef<DrawPointer | null>(null);
  const modeRef = useRef(mode);
  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);

  const controller = useMemo<DrawController>(() => {
    const update = (next: MeasureState) => {
      measureRef.current = next;
      setMeasure(next);
      useDrawStore.getState().setPreview(rulerPreview(next));
    };
    const pick = (p: DrawPointer) => {
      const view = useViewStore.getState();
      const tolerance = pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
      void api.drawTools
        .measureAt(p.rawX, p.rawY, tolerance)
        .then((reply) => setHit(reply.hit))
        .catch(() => setHit(null));
    };
    return {
      cursor: 'crosshair',
      pointerDown: (p) => {
        if (modeRef.current !== 'points') {
          pick(p);
          return;
        }
        pressRef.current = p;
        update(measurePress(measureRef.current, { x: p.x, y: p.y }));
      },
      pointerMove: (p) => {
        setCursor({ x: p.x, y: p.y });
        if (modeRef.current === 'points') {
          update(measureMove(measureRef.current, { x: p.x, y: p.y }));
        }
      },
      pointerUp: (p) => {
        const press = pressRef.current;
        pressRef.current = null;
        if (press === null) {
          return;
        }
        const pxPerMm = useViewStore.getState().camera.pxPerMm;
        const dragged = isDrag({ x: press.rawX, y: press.rawY }, { x: p.rawX, y: p.rawY }, pxPerMm);
        update(measureRelease(measureRef.current, { x: p.x, y: p.y }, dragged));
      },
      keyDown: (event) => {
        if (event.key === 'Escape') {
          update(EMPTY_MEASURE);
          setHit(undefined);
          return true;
        }
        return false;
      },
      cancel: () => {
        pressRef.current = null;
        update(EMPTY_MEASURE);
      },
    };
  }, []);
  useDrawController(controller);

  // The picked span or vector shows highlighted.
  const overlays = useMemo<PreviewOverlay[]>(() => {
    if (hit === undefined || hit === null || mode === 'points') {
      return [];
    }
    return [{ path: mode === 'span' ? spanPath(hit) : hit.path, tone: 'accent' }];
  }, [hit, mode]);
  useOverlays(overlays);

  const changeMode = (next: MeasureMode) => {
    controller.cancel();
    setHit(undefined);
    setMode(next);
  };

  const clear = () => {
    controller.cancel();
    setHit(undefined);
  };

  const rows = pointRows(measure, cursor, units);
  const picked =
    hit === undefined || hit === null
      ? null
      : mode === 'span'
        ? spanRows(hit, units)
        : contourRows(hit, units);
  return (
    <div className="measure-form">
      <ChoiceField label={text.mode} onChange={changeMode} options={MODES} value={mode} />
      <p className="measure-form__hint">{text.hint}</p>
      <section aria-live="polite" className="measure-form__group">
        <h3 className="measure-form__legend">{MODES.find((m) => m.value === mode)?.label}</h3>
        {mode === 'points' ? (
          rows.length === 0 ? (
            <p className="measure-form__empty">{text.waiting}</p>
          ) : (
            <Readout rows={rows} />
          )
        ) : picked !== null ? (
          <Readout rows={picked} />
        ) : (
          <p className="measure-form__empty">
            {hit === null ? text.noVector : mode === 'span' ? text.pickSpan : text.pickContour}
          </p>
        )}
      </section>
      <FormButtons
        onClose={closeForm}
        onPrimary={closeForm}
        onSecondary={clear}
        primaryLabel={strings.form.ok}
        secondaryLabel={text.clear}
      />
    </div>
  );
}
