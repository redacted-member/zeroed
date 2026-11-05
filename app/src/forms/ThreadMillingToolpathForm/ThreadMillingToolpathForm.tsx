import { useCallback, useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { ThreadMillingParams } from '../../bridge/cam';
import { lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  applyPreset,
  convertThreadUnits,
  holeCircles,
  threadLength,
  initialThreadState,
  minorDiameter,
  multiPoint,
  presetsFor,
  threadParams,
  threadStateFrom,
  threadToolWarning,
  validateThread,
  withTool,
  type ThreadField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './ThreadMillingToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { threadPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const KINDS: ReadonlyArray<Choice<ThreadMillingParams['kind']>> = [
  { value: 'internal', label: text.thread.internal },
  { value: 'external', label: text.thread.external },
];

const HANDS: ReadonlyArray<Choice<ThreadMillingParams['hand']>> = [
  { value: 'left', label: text.thread.left },
  { value: 'right', label: text.thread.right },
];

export function ThreadMillingToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialThreadState, convertThreadUnits);
  const load = useCallback(
    (params: ThreadMillingParams, name: string) =>
      setState(threadStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('threadMilling', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateThread(state, units, form.tool), [form.tool, state, units]);
  // A multi point cutter cuts only its own pitch.
  const multi = multiPoint(form.tool);
  useEffect(() => {
    setState((current) => withTool(current, form.tool, unitsRef.current));
  }, [form.tool, setState, unitsRef]);
  const nameId = useId();
  const presetId = useId();
  const unit = unitSuffix(units);
  const toolWarning = threadToolWarning(form.tool);
  const params = threadParams(state, units, form.tool);
  const circlesWhy = firstWhy(
    [state.kind !== 'internal', strings.why.internalOnly],
    [!form.hasVectors, text.needsSelection],
    [params === null, strings.why.fixValues],
  );
  const maxDepth = lengthValue(state.maxDepth, units);
  const length = maxDepth === null ? null : threadLength(maxDepth, form.tool);
  const entities = useDocumentStore((store) => store.entities);

  const createCircles = async () => {
    if (params === null) {
      return;
    }
    const chosen = entities.filter((e) => form.selectedIds.includes(e.id));
    const store = useDocumentStore.getState();
    const made: number[] = [];
    for (const circle of holeCircles(params, chosen)) {
      const id = await store.createPath(circle);
      if (id !== null) {
        made.push(id);
      }
    }
    if (made.length > 0) {
      await useDocumentStore.getState().select(made);
    }
  };

  // Typing a size by hand leaves the preset list on Custom.
  const set = (field: ThreadField, value: string) =>
    setState((current) => ({
      ...current,
      [field]: value,
      preset: field === 'pitch' || field === 'diameter' ? '' : current.preset,
    }));
  const errorFor = (field: ThreadField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="thread-toolpath" onSubmit={onSubmit}>
      <p className="thread-toolpath__hint">
        {form.editingName === null ? text.thread.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="thread-toolpath__notice">{text.needsSelection}</p> : null}
      <fieldset className="thread-toolpath__group">
        <legend className="thread-toolpath__legend">{text.depths}</legend>
        <NumberField
          error={errorFor('startDepth')}
          label={text.startDepth}
          onChange={(v) => set('startDepth', v)}
          unit={unit}
          value={state.startDepth}
        />
        <NumberField
          error={errorFor('maxDepth')}
          label={text.thread.maxDepth}
          onChange={(v) => set('maxDepth', v)}
          unit={unit}
          value={state.maxDepth}
        />
        <p className="thread-toolpath__readout">
          {text.thread.threadLength}:{' '}
          <output>
            {length === null ? '' : text.thread.threadLengthValue(lengthText(length, units), unit)}
          </output>
        </p>
        <p className="thread-toolpath__hint">{text.thread.threadLengthHint}</p>
      </fieldset>
      <fieldset className="thread-toolpath__group">
        <legend className="thread-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="threadMill" tool={form.tool} units={units} />
        {toolWarning === undefined ? null : (
          <p className="thread-toolpath__notice" role="status">
            {toolWarning}
          </p>
        )}
      </fieldset>
      <fieldset className="thread-toolpath__group">
        <legend className="thread-toolpath__legend">{text.thread.definition}</legend>
        <label className="thread-toolpath__row" htmlFor={presetId}>
          {text.thread.preset}
          <select
            className="thread-toolpath__select"
            id={presetId}
            onChange={(event) =>
              setState((current) => applyPreset(current, event.target.value, unitsRef.current))
            }
            value={state.preset}
          >
            <option value="">{text.thread.custom}</option>
            {presetsFor(units)
              .filter((preset) => multi === null || Math.abs(preset.pitch - multi.pitch) < 1e-6)
              .map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.id}
                </option>
              ))}
          </select>
        </label>
        <NumberField
          disabled={multi !== null}
          error={errorFor('pitch')}
          label={text.thread.pitch}
          onChange={(v) => set('pitch', v)}
          unit={unit}
          value={state.pitch}
        />
        <NumberField
          error={errorFor('diameter')}
          label={text.thread.diameter}
          onChange={(v) => set('diameter', v)}
          unit={unit}
          value={state.diameter}
        />
        <p className="thread-toolpath__readout">
          {text.thread.minor}:{' '}
          <output>
            {params === null ? '' : `${lengthText(minorDiameter(params), units)} ${unit}`}
          </output>
        </p>
        <NumberField
          error={errorFor('fitTolerance')}
          label={text.thread.fitTolerance}
          onChange={(v) => set('fitTolerance', v)}
          unit={unit}
          value={state.fitTolerance}
        />
        <button
          className="thread-toolpath__button"
          disabled={circlesWhy !== undefined}
          onClick={() => void createCircles()}
          data-tip={whyTip(text.thread.createCirclesHint, circlesWhy)}
          type="button"
        >
          <ButtonFace icon="drawCircle" label={text.thread.createCircles} />
        </button>
      </fieldset>
      <fieldset className="thread-toolpath__group">
        <legend className="thread-toolpath__legend">{text.thread.type}</legend>
        <HelperRow picture={<HelperPicture picture={threadPicture(state.kind)} />}>
          <ChoiceField
            label={text.thread.type}
            onChange={(kind) => setState((current) => ({ ...current, kind }))}
            options={KINDS}
            value={state.kind}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="thread-toolpath__group">
        <legend className="thread-toolpath__legend">{text.thread.direction}</legend>
        <ChoiceField
          label={text.thread.hand}
          onChange={(hand) => setState((current) => ({ ...current, hand }))}
          options={HANDS}
          value={state.hand}
        />
        <CheckField
          checked={state.cut === 'down'}
          label={text.thread.topToBottom}
          onChange={(down) => setState((current) => ({ ...current, cut: down ? 'down' : 'up' }))}
        />
      </fieldset>
      <CheckField
        checked={state.selectionOrder}
        label={text.selectionOrder}
        onChange={(selectionOrder) => setState((current) => ({ ...current, selectionOrder }))}
      />
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="thread-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="thread-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.thread.title}
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
