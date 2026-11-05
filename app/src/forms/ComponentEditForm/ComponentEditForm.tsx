import { useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { ComponentEditOp } from '../../bridge/model';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertEditUnits,
  editArgs,
  EDIT_OPS,
  initialEditState,
  validateEdit,
  type EditField,
} from './model';

import './ComponentEditForm.scss';

const text = strings.model.edit;

type MirrorAxis = 'horizontal' | 'vertical';

const AXES: ReadonlyArray<Choice<MirrorAxis>> = [
  { value: 'horizontal', label: text.leftRight },
  { value: 'vertical', label: text.topBottom },
];

const HINTS: Record<ComponentEditOp, string> = {
  smooth: text.smoothHint,
  draft: text.draftHint,
  tilt: text.tiltHint,
  fade: text.fadeHint,
  mirror: text.mirrorHint,
  flip: text.flipHint,
  scale: text.scaleHint,
  emboss: text.embossHint,
};

export function ComponentEditForm(): JSX.Element {
  const op = useModelStore((store) => store.editOp);
  const setEditOp = useModelStore((store) => store.setEditOp);
  const editComponent = useModelStore((store) => store.editComponent);
  const component = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialEditState, convertEditUnits);
  const [showErrors, setShowErrors] = useState(false);
  const bbox = component?.bbox;
  const errors = useMemo(() => validateEdit(op, state, units, bbox), [op, state, units, bbox]);
  const opId = useId();
  const unit = unitSuffix(units);
  const deg = strings.units.degrees;

  const set = (field: EditField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const field = (name: EditField, label: string, suffix?: string, step?: number) => (
    <NumberField
      error={showErrors ? errors[name] : undefined}
      label={label}
      onChange={(v) => set(name, v)}
      step={step}
      unit={suffix}
      value={state[name]}
    />
  );

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const edit = editArgs(op, state, units, bbox);
    if (edit !== null && component !== null) {
      void editComponent(component.id, edit);
    }
  };

  return (
    <form className="component-edit" onSubmit={onSubmit}>
      <p className="component-edit__hint">{text.hint}</p>
      {component === null ? (
        <p className="component-edit__notice">{text.needsComponent}</p>
      ) : (
        <p className="component-edit__target">{text.editing(component.name)}</p>
      )}
      <fieldset className="component-edit__group">
        <legend className="component-edit__legend">{text.operation}</legend>
        <label className="component-edit__op" htmlFor={opId}>
          {text.operation}
          <select
            className="component-edit__select"
            id={opId}
            onChange={(event) => setEditOp(event.target.value as ComponentEditOp)}
            value={op}
          >
            {EDIT_OPS.map((value) => (
              <option key={value} value={value}>
                {text[value]}
              </option>
            ))}
          </select>
        </label>
        <p className="component-edit__hint">{HINTS[op]}</p>
        {op === 'smooth' ? (
          <>
            {field('strength', text.strength, unit)}
            {field('iterations', text.iterations, undefined, 1)}
          </>
        ) : null}
        {op === 'draft' ? field('draftAngle', text.draftAngle, deg, 1) : null}
        {op === 'tilt' ? (
          <>
            {field('tiltAngle', text.tiltAngle, deg, 1)}
            {field('tiltDirection', text.direction, deg, 15)}
            <CheckField
              checked={state.pivotAtCentre}
              label={text.pivotCentre}
              onChange={(pivotAtCentre) => setState((current) => ({ ...current, pivotAtCentre }))}
            />
            {state.pivotAtCentre ? (
              showErrors && errors.pivotX !== undefined ? (
                <p className="component-edit__notice">{errors.pivotX}</p>
              ) : null
            ) : (
              <>
                {field('pivotX', text.pivotX, unit)}
                {field('pivotY', text.pivotY, unit)}
              </>
            )}
          </>
        ) : null}
        {op === 'fade' ? (
          <>
            {field('fadeDirection', text.direction, deg, 15)}
            {field('fadeStart', text.fadeStart, text.percent, 5)}
            {field('fadeEnd', text.fadeEnd, text.percent, 5)}
          </>
        ) : null}
        {op === 'mirror' ? (
          <ChoiceField
            label={text.mirrorAxis}
            onChange={(axis) =>
              setState((current) => ({ ...current, mirrorHorizontal: axis === 'horizontal' }))
            }
            options={AXES}
            value={state.mirrorHorizontal ? 'horizontal' : 'vertical'}
          />
        ) : null}
        {op === 'scale' ? (
          <>
            {field('scale', text.scaleFactor, undefined, 0.1)}
            {field('baseOffset', text.baseOffset, unit)}
          </>
        ) : null}
        {op === 'emboss' ? field('maxHeight', text.maxHeight, unit) : null}
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={component === null ? text.needsComponent : undefined}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
