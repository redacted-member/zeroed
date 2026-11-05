import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/api';
import type { CurvePosition, OnCurve, Path, Point } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useTextPreview } from '../../lib/useTextPreview';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  convertOnCurveUnits,
  curveOf,
  initialOnCurveState,
  offsetError,
  pickTarget,
  slideTo,
  SPACING_MAX,
  SPACING_MIN,
  stateFromCurve,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './TextOnCurveForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { textOnCurvePicture } from '../HelperPicture/pictures/drawing';

const text = strings.text.onCurve;

const SIZES: ReadonlyArray<Choice<'maintain' | 'fill'>> = [
  { value: 'maintain', label: text.maintain },
  { value: 'fill', label: text.fill },
];

const POSITIONS: ReadonlyArray<Choice<CurvePosition>> = [
  { value: 'above', label: text.above },
  { value: 'on', label: text.on },
  { value: 'below', label: text.below },
];

const ALIGNMENTS: ReadonlyArray<Choice<OnCurve['align']>> = [
  { value: 'left', label: text.left },
  { value: 'centre', label: text.centre },
  { value: 'right', label: text.right },
];

const ORIENTATIONS: ReadonlyArray<Choice<'curve' | 'vertical'>> = [
  { value: 'curve', label: text.alignToCurve },
  { value: 'vertical', label: text.keepVertical },
];

/** Text On A Curve: sets the selected text along the selected vector, or
 * edits text already on a curve; a drag in the view slides it along. */
export function TextOnCurveForm(): JSX.Element {
  const textId = useDocumentStore(
    (store) => pickTarget(store.selection.ids, store.entities)?.textId ?? null,
  );
  const pathId = useDocumentStore(
    (store) => pickTarget(store.selection.ids, store.entities)?.pathId ?? null,
  );
  const target = textId === null || pathId === null ? null : { textId, pathId };
  const setTextOnCurve = useDocumentStore((store) => store.setTextOnCurve);
  const closeForm = useUiStore((store) => store.closeForm);
  const place = useFormState(initialOnCurveState, convertOnCurveUnits);
  const units = place.units;
  const setPlace = place.setState;
  const [baseSpacing, setBaseSpacing] = useState(100);
  const curvePath = useRef<Path | null>(null);
  const stateRef = useRef(place.state);
  useEffect(() => {
    stateRef.current = place.state;
  }, [place.state]);

  // Text already on a curve opens with its settings.
  useEffect(() => {
    if (textId === null) {
      return;
    }
    let live = true;
    void api.entities.get(textId).then((entity) => {
      const kept = entity.kind === 'text' ? entity.spec.onCurve : undefined;
      if (live && kept !== undefined) {
        setBaseSpacing(kept.spacing);
        setPlace(stateFromCurve(kept, place.unitsRef.current));
      }
    });
    return () => {
      live = false;
    };
  }, [textId, setPlace, place.unitsRef]);

  useEffect(() => {
    curvePath.current = null;
    if (pathId === null) {
      return;
    }
    let live = true;
    void api.entities.get(pathId).then((entity) => {
      if (live && entity.kind === 'path') {
        curvePath.current = entity.path;
      }
    });
    return () => {
      live = false;
    };
  }, [pathId]);

  const curve = pathId === null ? null : curveOf(place.state, units, pathId, baseSpacing);
  useTextPreview(textId === null || curve === null ? null : { kind: 'onCurve', textId, curve });

  const pressRef = useRef<{ at: Point; startAt: number } | null>(null);
  const controller = useMemo<DrawController>(() => {
    return {
      cursor: 'ew-resize',
      pointerDown: (p) => {
        pressRef.current = { at: { x: p.rawX, y: p.rawY }, startAt: stateRef.current.startAt };
      },
      pointerMove: (p) => {
        const path = curvePath.current;
        const press = pressRef.current;
        if (press === null || path === null) {
          return;
        }
        const startAt = slideTo(
          path,
          press.at,
          { x: p.rawX, y: p.rawY },
          press.startAt,
          stateRef.current.otherSide,
        );
        setPlace((current) => ({ ...current, startAt }));
      },
      pointerUp: () => {
        pressRef.current = null;
      },
      cancel: () => {
        pressRef.current = null;
      },
    };
  }, [setPlace]);
  useDrawController(controller);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (textId !== null && curve !== null) {
      void setTextOnCurve({ textId, curve });
    }
  };

  const state = place.state;
  return (
    <form className="text-on-curve-form" onSubmit={onSubmit}>
      <p className="text-on-curve-form__hint">{text.hint}</p>
      {target === null ? <p className="text-on-curve-form__notice">{text.needs}</p> : null}
      <ChoiceField
        label={text.size}
        onChange={(size) => setPlace((current) => ({ ...current, fill: size === 'fill' }))}
        options={SIZES}
        value={state.fill ? 'fill' : 'maintain'}
      />
      <SliderField
        label={text.spacing}
        max={SPACING_MAX}
        min={SPACING_MIN}
        onChange={(spacing) => setPlace((current) => ({ ...current, spacing }))}
        unit="%"
        value={state.spacing}
      />
      <HelperRow picture={<HelperPicture picture={textOnCurvePicture(state.position)} />}>
        <ChoiceField
          label={text.position}
          onChange={(position) => setPlace((current) => ({ ...current, position }))}
          options={POSITIONS}
          value={state.position}
        />
      </HelperRow>
      <CheckField
        checked={state.otherSide}
        label={text.otherSide}
        onChange={(otherSide) => setPlace((current) => ({ ...current, otherSide }))}
      />
      <NumberField
        error={offsetError(state, units)}
        label={text.offset}
        onChange={(offset) => setPlace((current) => ({ ...current, offset }))}
        unit={unitSuffix(units)}
        value={state.offset}
      />
      <ChoiceField
        disabled={state.fill}
        label={text.alignment}
        onChange={(align) => setPlace((current) => ({ ...current, align, startAt: 0 }))}
        options={ALIGNMENTS}
        value={state.align}
      />
      <ChoiceField
        label={text.orientation}
        onChange={(value) =>
          setPlace((current) => ({ ...current, keepVertical: value === 'vertical' }))
        }
        options={ORIENTATIONS}
        value={state.keepVertical ? 'vertical' : 'curve'}
      />
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [target === null, text.needs],
          [curve === null, strings.why.fixValues],
        )}
        primaryLabel={strings.form.apply}
        submit
      />
    </form>
  );
}
