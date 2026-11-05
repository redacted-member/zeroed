import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import type { Point } from '../../bridge/document';
import type { QuickEntry } from '../../lib/quickKeys';
import { recogniseRectangle, useShapeEditor, type RectangleShape } from '../../lib/shapeEdit';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { circlePath } from '../../lib/geometry';
import { useDrawController, useOverlays, usePreview } from '../../lib/useDrawController';
import { ShapeDrag } from '../../lib/useShapeDrag';
import type { CornerType } from '../../lib/shapes';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import type { DrawController, DrawPointer, PreviewOverlay } from '../../state/drawStore';
import { useViewStore } from '../../state/viewStore';
import { AnchorPicker } from '../AnchorPicker/AnchorPicker';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertRectangleUnits,
  initialRectangleState,
  rectangleAt,
  rectangleFromDrag,
  rectangleFromQuick,
  rectanglePath,
  validateRectangle,
  type RectangleField,
  type RectangleFormState,
  rectangleFromShape,
  rectanglePlacedOn,
  radiusFromDrag,
  radiusHandles,
  type RadiusHandle,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DrawRectangleForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { cornerPicture } from '../HelperPicture/pictures/drawing';

const CORNERS: ReadonlyArray<{ value: CornerType; label: string }> = [
  { value: 'none', label: strings.createVectors.rectangle.cornerNone },
  { value: 'external', label: strings.createVectors.rectangle.cornerExternal },
  { value: 'internal', label: strings.createVectors.rectangle.cornerInternal },
];

/** Size of a corner radius handle and how near, px, a press picks one up. */
const RADIUS_HANDLE_PX = 7;
const RADIUS_HIT_PX = 8;

export function DrawRectangleForm(): JSX.Element {
  const units = useDocumentStore((state) => state.job.units);
  const createPath = useDocumentStore((state) => state.createPath);
  const closeForm = useUiStore((state) => state.closeForm);
  const text = strings.createVectors.rectangle;

  const [state, setState] = useState<RectangleFormState>(() => initialRectangleState(units));
  const [showErrors, setShowErrors] = useState(false);
  // Live copies for the pointer handlers, which run outside of render.
  const stateRef = useRef(state);
  const unitsRef = useRef(units);
  const dragRef = useRef(new ShapeDrag());

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const editor = useShapeEditor<RectangleFormState, RectangleShape>({
    recognise: recogniseRectangle,
    fill: rectangleFromShape,
    place: rectanglePlacedOn,
    build: rectanglePath,
    setState,
    stateRef,
    units,
    unitsRef,
  });

  useEffect(() => {
    const previous = unitsRef.current;
    if (previous !== units) {
      setState((current) => convertRectangleUnits(current, previous, units));
      unitsRef.current = units;
    }
  }, [units]);

  const errors = useMemo(() => validateRectangle(state, units), [state, units]);
  const path = useMemo(() => rectanglePath(state, units), [state, units]);
  usePreview(useMemo(() => (path === null ? [] : [path]), [path]));

  // The four green corner radius handles on the preview.
  const pxPerMm = useViewStore((store) => store.camera.pxPerMm);
  const handles = useMemo(() => radiusHandles(state, units), [state, units]);
  const handlesRef = useRef(handles);
  useEffect(() => {
    handlesRef.current = handles;
  }, [handles]);
  const pxRef = useRef(pxPerMm);
  useEffect(() => {
    pxRef.current = pxPerMm;
  }, [pxPerMm]);
  const radiusDrag = useRef<RadiusHandle | null>(null);
  const overlays = useMemo<PreviewOverlay[]>(
    () =>
      handles.map((handle) => ({
        path: circlePath(handle.at.x, handle.at.y, RADIUS_HANDLE_PX / 2 / pxPerMm),
        tone: 'second',
        fill: true,
      })),
    [handles, pxPerMm],
  );
  useOverlays(overlays);

  const create = useCallback(async () => {
    setShowErrors(true);
    const current = rectanglePath(stateRef.current, unitsRef.current);
    if (current === null) {
      return;
    }
    await createPath(current);
  }, [createPath]);

  // A finished drag makes the rectangle from its start to the pointer.
  const commitDrag = useCallback(
    (from: Point, p: DrawPointer) => {
      const next = rectangleFromDrag(
        stateRef.current,
        unitsRef.current,
        from,
        p,
        p.ctrl,
        p.alt === true,
      );
      setState(next);
      const built = rectanglePath(next, unitsRef.current);
      if (built !== null) {
        void createPath(built);
      }
    },
    [createPath],
  );

  const controller = useMemo<DrawController>(
    () => ({
      pointerDown: (p: DrawPointer) => {
        const reach = RADIUS_HIT_PX / pxRef.current;
        const grabbed = handlesRef.current.find(
          (handle) => Math.hypot(p.rawX - handle.at.x, p.rawY - handle.at.y) <= reach,
        );
        if (grabbed !== undefined) {
          radiusDrag.current = grabbed;
          return;
        }
        dragRef.current.down(p);
      },
      pointerMove: (p: DrawPointer) => {
        const held = radiusDrag.current;
        if (held !== null) {
          setState((current) => radiusFromDrag(current, unitsRef.current, held, p));
          return;
        }
        const step = dragRef.current.move(p);
        if (step?.kind === 'drag') {
          const from = step.from;
          setState((current) =>
            rectangleFromDrag(current, unitsRef.current, from, p, p.ctrl, p.alt === true),
          );
        }
      },
      pointerUp: (p: DrawPointer) => {
        const held = radiusDrag.current;
        if (held !== null) {
          radiusDrag.current = null;
          setState((current) => radiusFromDrag(current, unitsRef.current, held, p));
          return;
        }
        const step = dragRef.current.up(p);
        if (step?.kind === 'commit') {
          commitDrag(step.from, step.pointer);
        } else if (step?.kind === 'click') {
          editor.click({ x: p.x, y: p.y }, p, () =>
            setState((current) => rectangleAt(current, unitsRef.current, p)),
          );
        }
      },
      keyDown: (event: KeyboardEvent) => {
        if (event.key !== 'Enter') {
          return false;
        }
        // Enter during a drag finishes it there; the release then makes nothing.
        const step = dragRef.current.enter();
        if (step?.kind === 'commit') {
          commitDrag(step.from, step.pointer);
        } else {
          void create();
        }
        return true;
      },
      quickEntry: (entry: QuickEntry) => {
        const at = dragRef.current.quick();
        if (at === null) {
          return false;
        }
        const next = rectangleFromQuick(
          stateRef.current,
          unitsRef.current,
          at.from,
          at.cursor,
          entry,
        );
        if (next === null) {
          return false;
        }
        dragRef.current.reset();
        setState(next);
        const built = rectanglePath(next, unitsRef.current);
        if (built !== null) {
          void createPath(built);
        }
        return true;
      },
      cancel: () => {
        radiusDrag.current = null;
        dragRef.current.reset();
      },
    }),
    [create, commitDrag, createPath, editor],
  );
  useDrawController(controller);

  const setField = (field: RectangleField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: RectangleField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  return (
    <form className="draw-rectangle" onSubmit={onSubmit}>
      <p className="draw-rectangle__hint">{text.hint}</p>
      <fieldset className="draw-rectangle__group">
        <legend className="draw-rectangle__legend">{strings.form.anchor}</legend>
        <AnchorPicker
          label={strings.form.anchor}
          onChange={(anchor) => setState((current) => ({ ...current, anchor }))}
          value={state.anchor}
        />
        <NumberField
          error={errorFor('x')}
          label="X"
          onChange={(v) => setField('x', v)}
          unit={unit}
          value={state.x}
        />
        <NumberField
          error={errorFor('y')}
          label="Y"
          onChange={(v) => setField('y', v)}
          unit={unit}
          value={state.y}
        />
      </fieldset>
      <fieldset className="draw-rectangle__group">
        <legend className="draw-rectangle__legend">{text.cornerType}</legend>
        <HelperRow picture={<HelperPicture picture={cornerPicture(state.corner)} />}>
          <div className="draw-rectangle__choices">
            {CORNERS.map((corner) => (
              <label className="draw-rectangle__choice" key={corner.value}>
                <input
                  checked={state.corner === corner.value}
                  name="corner"
                  onChange={() => setState((current) => ({ ...current, corner: corner.value }))}
                  type="radio"
                  value={corner.value}
                />
                {corner.label}
              </label>
            ))}
          </div>
          <NumberField
            disabled={state.corner === 'none'}
            error={errorFor('radius')}
            label={text.radius}
            onChange={(v) => setField('radius', v)}
            unit={unit}
            value={state.radius}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="draw-rectangle__group">
        <legend className="draw-rectangle__legend">{text.size}</legend>
        <NumberField
          error={errorFor('width')}
          label={text.width}
          onChange={(v) => setField('width', v)}
          unit={unit}
          value={state.width}
        />
        <NumberField
          error={errorFor('height')}
          label={text.height}
          onChange={(v) => setField('height', v)}
          unit={unit}
          value={state.height}
        />
      </fieldset>
      {editor.count > 0 ? (
        <>
          <p className="draw-rectangle__hint" role="status">
            {strings.createVectors.editing(editor.count)}
          </p>
          <button
            className="draw-rectangle__button"
            onClick={() => void editor.apply()}
            type="button"
            data-tip={strings.form.apply}
          >
            <ButtonFace icon="formApply" label={strings.form.apply} />
          </button>
        </>
      ) : null}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void create()}
        primaryLabel={strings.form.create}
        submit
      />
    </form>
  );
}
