import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Bbox } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { circlePath } from '../../lib/geometry';
import { unitSuffix } from '../../lib/units';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController, PreviewOverlay } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { jobBounds } from '../../views/View2D/camera';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  centrePoint,
  convertUnwrapper,
  customAt,
  initialUnwrapper,
  unwrapBlocker,
  type CentreChoice,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './VectorUnwrapperForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { unwrap } from '../HelperPicture/pictures/drawing';

const text = strings.vectorUnwrapper;

const CENTRES: ReadonlyArray<Choice<CentreChoice>> = [
  { value: 'contour', label: text.centreContour },
  { value: 'workspace', label: text.centreWorkspace },
  { value: 'custom', label: text.centreCustom },
];

/** Handle radius and pick distance, in screen pixels. */
const HANDLE_PX = 6;
const PICK_PX = 10;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function VectorUnwrapperForm(): JSX.Element {
  const job = useDocumentStore((store) => store.job);
  const selection = useDocumentStore((store) => store.selection);
  const entities = useDocumentStore((store) => store.entities);
  const closeForm = useUiStore((store) => store.closeForm);
  const pxPerMm = useViewStore((store) => store.camera.pxPerMm);
  const form = useFormState(
    (units) => initialUnwrapper(units, useDocumentStore.getState().selection.bbox),
    convertUnwrapper,
  );
  const { state, setState, units } = form;
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const only = selection.ids.length === 1 ? selection.ids[0] : undefined;
  const contour = entities.find((e) => e.id === only);
  const closed = contour !== undefined && contour.kind === 'path' && contour.closed;
  const blocker = unwrapBlocker(job, selection.ids.length, closed);
  const workspace = useMemo<Bbox>(() => {
    const b = jobBounds(job);
    return [b.minX, b.minY, b.maxX, b.maxY];
  }, [job]);
  const centre = centrePoint(state, selection.bbox, workspace, units);

  const cx = centre?.x ?? null;
  const cy = centre?.y ?? null;
  const overlays = useMemo<PreviewOverlay[]>(
    () =>
      cx === null || cy === null
        ? []
        : [{ path: circlePath(cx, cy, HANDLE_PX / pxPerMm), tone: 'accent', fill: true }],
    [cx, cy, pxPerMm],
  );
  useOverlays(overlays);

  // The centre handle drags; a drag makes the point a custom one.
  const centreRef = useRef(centre);
  const dragging = useRef(false);
  useEffect(() => {
    centreRef.current = cx === null || cy === null ? null : { x: cx, y: cy };
  }, [cx, cy]);
  const controller = useMemo<DrawController>(() => {
    const move = (p: { x: number; y: number }) =>
      setState((current) => customAt(current, p, useDocumentStore.getState().job.units));
    return {
      cursor: 'crosshair',
      pointerDown: (p) => {
        const at = centreRef.current;
        const reach = PICK_PX / useViewStore.getState().camera.pxPerMm;
        dragging.current = at !== null && Math.hypot(p.rawX - at.x, p.rawY - at.y) <= reach;
      },
      pointerMove: (p) => {
        if (dragging.current) {
          move(p);
        }
      },
      pointerUp: (p) => {
        if (dragging.current) {
          move(p);
        }
        dragging.current = false;
      },
      cancel: () => {
        dragging.current = false;
      },
    };
  }, [setState]);
  useDrawController(controller);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (blocker !== null || busy || only === undefined || centre === null) {
      return;
    }
    setBusy(true);
    setStatus(null);
    setFailure(null);
    api.job
      .unwrapContour(only, centre, state.refine)
      .then(() => setStatus(text.applied))
      .catch((error: unknown) => setFailure(message(error)))
      .finally(() => {
        void useDocumentStore
          .getState()
          .refresh()
          .finally(() => setBusy(false));
      });
  };

  const unit = unitSuffix(units);
  return (
    <form className="vector-unwrapper-form" onSubmit={onSubmit}>
      <p className="vector-unwrapper-form__hint">{text.hint}</p>
      {blocker === null ? null : <p className="vector-unwrapper-form__notice">{blocker}</p>}
      <HelperRow picture={<HelperPicture picture={unwrap} />}>
        <ChoiceField
          label={text.centre}
          onChange={(value) => setState((current) => ({ ...current, centre: value }))}
          options={CENTRES}
          value={state.centre}
        />
      </HelperRow>
      {state.centre === 'custom' ? (
        <>
          <NumberField
            label={text.x}
            onChange={(x) => setState((current) => ({ ...current, x }))}
            unit={unit}
            value={state.x}
          />
          <NumberField
            label={text.y}
            onChange={(y) => setState((current) => ({ ...current, y }))}
            unit={unit}
            value={state.y}
          />
        </>
      ) : null}
      <p className="vector-unwrapper-form__hint">{text.customHint}</p>
      <CheckField
        checked={state.refine}
        label={text.refine}
        onChange={(refine) => setState((current) => ({ ...current, refine }))}
      />
      {status === null ? null : (
        <p className="vector-unwrapper-form__status" role="status">
          {status}
        </p>
      )}
      {failure === null ? null : (
        <p className="vector-unwrapper-form__notice" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [busy, strings.why.busy],
          [blocker !== null, blocker ?? ''],
          [centre === null, strings.why.fixValues],
        )}
        primaryLabel={text.apply}
        submit
      />
    </form>
  );
}
