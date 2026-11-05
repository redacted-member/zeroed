import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { CutDirection, PrismDirection, PrismParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { CheckField } from '../CheckField/CheckField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertPrismUnits,
  initialPrismState,
  prismDirectionHint,
  prismParams,
  prismStateFrom,
  prismToolWarning,
  validatePrism,
  withFullDepth,
  withPrismDepth,
  type PrismField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './PrismToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { prismPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const DIRECTIONS: ReadonlyArray<Choice<PrismDirection>> = [
  { value: 'outside', label: text.prism.outside },
  { value: 'inside', label: text.prism.inside },
];

const CUT_DIRECTIONS: ReadonlyArray<Choice<CutDirection>> = [
  { value: 'climb', label: text.climb },
  { value: 'conventional', label: text.conventional },
];

export function PrismToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialPrismState, convertPrismUnits);
  const load = useCallback(
    (params: PrismParams, name: string) => setState(prismStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('prism', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validatePrism(state, units), [state, units]);
  const nameId = useId();
  const directionHintId = useId();
  const unit = unitSuffix(units);
  const toolWarning = prismToolWarning(form.tool);
  const [depthError, setDepthError] = useState<string | null>(null);

  const setFullDepth = async () => {
    if (form.tool === null) {
      return;
    }
    try {
      const reply = await api.cam.prismDepth(form.tool, form.selectedIds);
      setDepthError(null);
      setState((current) => withFullDepth(current, reply.depth, unitsRef.current));
    } catch (error) {
      setDepthError(error instanceof Error ? error.message : String(error));
    }
  };

  const set = (field: PrismField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: PrismField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = prismParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="prism-toolpath" onSubmit={onSubmit}>
      <p className="prism-toolpath__hint">
        {form.editingName === null ? text.prism.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="prism-toolpath__notice">{text.needsSelection}</p> : null}
      {state.direction === 'inside' ? (
        // Only toolpaths made before the current layout carve inside.
        <fieldset aria-describedby={directionHintId} className="prism-toolpath__group">
          <legend className="prism-toolpath__legend">{text.prism.direction}</legend>
          <ChoiceField
            label={text.prism.direction}
            onChange={(direction) => setState((current) => ({ ...current, direction }))}
            options={DIRECTIONS}
            value={state.direction}
          />
          <p className="prism-toolpath__hint" id={directionHintId}>
            {prismDirectionHint(state.direction)}
          </p>
        </fieldset>
      ) : null}
      <fieldset className="prism-toolpath__group">
        <legend className="prism-toolpath__legend">{text.depths}</legend>
        <HelperRow picture={<HelperPicture picture={prismPicture(state.direction)} />}>
          <NumberField
            error={errorFor('startDepth')}
            label={text.startDepth}
            onChange={(v) => set('startDepth', v)}
            unit={unit}
            value={state.startDepth}
          />
          <NumberField
            error={errorFor('maxDepth')}
            label={text.prism.maxDepth}
            onChange={(v) => setState((current) => withPrismDepth(current, v))}
            placeholder={text.prism.fullPrism}
            unit={unit}
            value={state.maxDepth}
          />
        </HelperRow>
        <button
          className="prism-toolpath__button"
          disabled={form.calculateWhy !== undefined}
          onClick={() => void setFullDepth()}
          data-tip={whyTip(text.prism.fullDepthHint, form.calculateWhy)}
          type="button"
        >
          <ButtonFace icon="setValue" label={text.prism.fullDepth} />
        </button>
        {depthError === null ? null : (
          <p className="prism-toolpath__notice" role="alert">
            {depthError}
          </p>
        )}
      </fieldset>
      <fieldset className="prism-toolpath__group">
        <legend className="prism-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="vBit" tool={form.tool} units={units} />
        <p className="prism-toolpath__hint">{text.prism.passHint}</p>
        {toolWarning === undefined ? null : (
          <p className="prism-toolpath__notice" role="status">
            {toolWarning}
          </p>
        )}
      </fieldset>
      <ChoiceField
        label={text.cutDirection}
        onChange={(cutDirection) => setState((current) => ({ ...current, cutDirection }))}
        options={CUT_DIRECTIONS}
        value={state.cutDirection}
      />
      <CheckField
        checked={state.selectionOrder}
        label={strings.toolpaths.selectionOrder}
        onChange={(selectionOrder) => setState((current) => ({ ...current, selectionOrder }))}
      />
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="prism-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="prism-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.prism.title}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={form.calculateWhy}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
