import { useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertTransformUnits,
  initialTransformState,
  setLinked,
  setTransformField,
  transformParams,
  transformPreview,
  validateTransform,
  type TransformField,
} from './model';

import './ComponentTransformForm.scss';

const text = strings.model.transform;

type PivotChoice = 'centre' | 'point';

const PIVOTS: ReadonlyArray<Choice<PivotChoice>> = [
  { value: 'centre', label: text.pivotCentre },
  { value: 'point', label: text.pivotPoint },
];

export function ComponentTransformForm(): JSX.Element {
  const transformComponent = useModelStore((store) => store.transformComponent);
  const component = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialTransformState,
    convertTransformUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateTransform(state, units), [state, units]);
  const bbox = component?.bbox;
  usePreview(useMemo(() => transformPreview(state, units, bbox), [state, units, bbox]));
  const unit = unitSuffix(units);

  const field = (name: TransformField, label: string, suffix?: string, step?: number) => (
    <NumberField
      error={showErrors ? errors[name] : undefined}
      label={label}
      onChange={(value) => setState((current) => setTransformField(current, name, value))}
      step={step}
      unit={suffix}
      value={state[name]}
    />
  );

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = transformParams(state, units);
    if (params !== null && component !== null) {
      void transformComponent(component.id, params);
    }
  };

  return (
    <form className="component-transform" onSubmit={onSubmit}>
      <p className="component-transform__hint">{text.hint}</p>
      {component === null ? (
        <p className="component-transform__notice">{text.needsComponent}</p>
      ) : (
        <p className="component-transform__target">{text.editing(component.name)}</p>
      )}
      <fieldset className="component-transform__group">
        <legend className="component-transform__legend">{text.move}</legend>
        {field('dx', text.dx, unit)}
        {field('dy', text.dy, unit)}
      </fieldset>
      <fieldset className="component-transform__group">
        <legend className="component-transform__legend">{text.scale}</legend>
        {field('scaleX', text.scaleX, text.percent, 1)}
        {field('scaleY', text.scaleY, text.percent, 1)}
        <CheckField
          checked={state.linked}
          label={text.link}
          onChange={(linked) => setState((current) => setLinked(current, linked))}
        />
      </fieldset>
      <fieldset className="component-transform__group">
        <legend className="component-transform__legend">{text.rotate}</legend>
        {field('angle', text.angle, strings.units.degrees, 15)}
        <p className="component-transform__hint">{text.angleHint}</p>
      </fieldset>
      <fieldset className="component-transform__group">
        <legend className="component-transform__legend">{text.pivot}</legend>
        <ChoiceField
          label={text.pivot}
          onChange={(pivot) =>
            setState((current) => ({ ...current, pivotAtCentre: pivot === 'centre' }))
          }
          options={PIVOTS}
          value={state.pivotAtCentre ? 'centre' : 'point'}
        />
        {state.pivotAtCentre ? null : (
          <>
            {field('pivotX', text.pivotX, unit)}
            {field('pivotY', text.pivotY, unit)}
          </>
        )}
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        onSecondary={() => {
          setShowErrors(false);
          setState(initialTransformState(unitsRef.current));
        }}
        primaryWhy={component === null ? text.needsComponent : undefined}
        primaryLabel={text.action}
        secondaryLabel={text.reset}
        submit
      />
    </form>
  );
}
