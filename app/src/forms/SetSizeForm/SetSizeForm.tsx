import { useEffect, useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { selectionFrame } from '../../lib/transformMath';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { useUiStore } from '../../state/uiStore';
import { AnchorPicker } from '../AnchorPicker/AnchorPicker';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertSetSizeUnits,
  individualMatrices,
  initialSetSizeState,
  setSizeField,
  setSizeMatrixFromState,
  setSizeModeIcon,
  sizeStateFromSelection,
  validateSetSize,
  type SetSizeField,
} from './model';

import './SetSizeForm.scss';

export function SetSizeForm(): JSX.Element {
  // The mode pictures load with the menu and form icons after start up.
  useExtraIcons();
  const selection = useDocumentStore((store) => store.selection);
  const entities = useDocumentStore((store) => store.entities);
  const transformSelection = useDocumentStore((store) => store.transformSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const componentCount = useModelStore((store) => pickedComponents(store).length);
  const text = strings.transform.setSize;
  const { state, setState, units, unitsRef } = useFormState(
    initialSetSizeState,
    convertSetSizeUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const frame = useMemo(() => selectionFrame(selection), [selection]);
  const bbox = frame?.extent ?? null;

  useEffect(() => {
    setState((current) => sizeStateFromSelection(current, unitsRef.current, bbox));
  }, [bbox, setState, unitsRef]);

  const errors = useMemo(() => validateSetSize(state, units), [state, units]);

  const apply = async () => {
    setShowErrors(true);
    if (frame === null) {
      return;
    }
    if (state.individually && selection.bbox !== null) {
      const items = entities
        .filter((e) => selection.ids.includes(e.id))
        .map((e) => ({ id: e.id, bbox: e.bbox }));
      const each = individualMatrices(state, units, selection.bbox, items);
      for (const item of each ?? []) {
        await transformSelection(item.matrix, [item.id]);
      }
      return;
    }
    const matrix = setSizeMatrixFromState(state, units, frame);
    if (matrix !== null) {
      await transformSelection(matrix, undefined, state.autoScaleZ);
    }
  };

  const setField = (field: SetSizeField, value: string) =>
    setState((current) => setSizeField(current, unitsRef.current, bbox, field, value));
  const errorFor = (field: SetSizeField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void apply();
  };

  return (
    <form className="set-size-form" onSubmit={onSubmit}>
      {bbox === null ? (
        <p className="set-size-form__notice">{strings.transform.noSelection}</p>
      ) : null}
      <fieldset className="set-size-form__group">
        <legend className="set-size-form__legend">{text.mode}</legend>
        <div className="set-size-form__mode">
          <span aria-hidden="true" className="set-size-form__picture">
            {toolIcon(setSizeModeIcon(state.individually))}
          </span>
          <div className="set-size-form__choices">
            <label className="set-size-form__choice">
              <input
                checked={!state.individually}
                name="setSizeMode"
                onChange={() => setState((current) => ({ ...current, individually: false }))}
                type="radio"
              />
              {text.asOne}
            </label>
            <label className="set-size-form__choice">
              <input
                checked={state.individually}
                name="setSizeMode"
                onChange={() => setState((current) => ({ ...current, individually: true }))}
                type="radio"
              />
              {text.individually}
            </label>
          </div>
        </div>
      </fieldset>
      <fieldset className="set-size-form__group">
        <legend className="set-size-form__legend">{strings.form.anchor}</legend>
        <AnchorPicker
          label={strings.form.anchor}
          onChange={(anchor) => setState((current) => ({ ...current, anchor }))}
          positions="nine"
          value={state.anchor}
        />
      </fieldset>
      <fieldset className="set-size-form__group">
        <legend className="set-size-form__legend">{text.title}</legend>
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
        <label className="set-size-form__choice">
          <input
            checked={state.link}
            onChange={(event) =>
              setState((current) => ({ ...current, link: event.target.checked }))
            }
            type="checkbox"
          />
          {text.link}
        </label>
      </fieldset>
      <CheckField
        checked={state.autoScaleZ}
        disabled={componentCount === 0}
        label={text.autoScaleZ}
        onChange={(autoScaleZ) => setState((current) => ({ ...current, autoScaleZ }))}
      />
      {componentCount > 0 ? <p className="set-size-form__hint">{text.autoScaleZHint}</p> : null}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void apply()}
        primaryWhy={bbox === null ? strings.transform.noSelection : undefined}
        primaryLabel={strings.form.apply}
        submit
      />
    </form>
  );
}
