import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { EngraveFill, EngraveParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { ImmediateOutput } from '../ImmediateOutput/ImmediateOutput';
import { NumberField } from '../NumberField/NumberField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertEngraveUnits,
  engraveParams,
  engraveStateFrom,
  initialEngraveState,
  validateEngrave,
  type EngraveField,
} from './model';

import './QuickEngraveToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { engravePicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const MODES: ReadonlyArray<Choice<EngraveParams['mode']>> = [
  { value: 'outline', label: text.engrave.outline },
  { value: 'fill', label: text.engrave.fill },
];

type FillKind = 'offset' | 'hatch';

const FILLS: ReadonlyArray<Choice<FillKind>> = [
  { value: 'offset', label: text.engrave.offset },
  { value: 'hatch', label: text.engrave.hatch },
];

/** Offset, or Hatch with or without Cross Hatch, as the kernel's pattern. */
function patternFor(kind: FillKind, cross: boolean): EngraveFill {
  if (kind === 'offset') {
    return 'offset';
  }
  return cross ? 'crossHatch' : 'raster';
}

export function QuickEngraveToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialEngraveState,
    convertEngraveUnits,
  );
  const load = useCallback(
    (params: EngraveParams, name: string) =>
      setState(engraveStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('quickEngrave', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  // The toolpath this form calculated last: Immediate Output saves it, and
  // Calculate again recalculates it, as the form stays open.
  const [lastId, setLastId] = useState<number | null>(() => useUiStore.getState().editingToolpath);
  const errors = useMemo(() => validateEngrave(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const fill = state.mode === 'fill';
  const drag = form.tool?.toolType.kind === 'diamondDrag';

  const set = (field: EngraveField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: EngraveField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = engraveParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name, { keepOpen: true }).then((ok) => {
        const id = useToolpathStore.getState().selectedId;
        if (ok && id !== null) {
          setLastId(id);
          useUiStore.setState({ editingToolpath: id });
        }
      });
    }
  };

  return (
    <form className="quick-engrave" onSubmit={onSubmit}>
      <p className="quick-engrave__hint">
        {form.editingName === null ? text.engrave.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="quick-engrave__notice">{text.needsSelection}</p> : null}
      <fieldset className="quick-engrave__group">
        <legend className="quick-engrave__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="engravingTool" tool={form.tool} units={units} />
      </fieldset>
      <NumberField
        disabled={state.noseCone}
        error={errorFor('depth')}
        label={text.engrave.depth}
        onChange={(v) => set('depth', v)}
        unit={unit}
        value={state.depth}
      />
      {drag ? <p className="quick-engrave__hint">{text.engrave.dragHint}</p> : null}
      <HelperRow picture={<HelperPicture picture={engravePicture(state.mode)} />}>
        <fieldset className="quick-engrave__group" aria-label={text.engrave.mode}>
          <ChoiceField
            label={text.engrave.mode}
            onChange={(mode) => setState((current) => ({ ...current, mode }))}
            options={MODES}
            value={state.mode}
          />
          <NumberField
            disabled={!fill}
            error={errorFor('stepover')}
            label={text.engrave.stepover}
            onChange={(v) => set('stepover', v)}
            unit={unit}
            value={state.stepover}
          />
          <ChoiceField
            disabled={!fill}
            label={text.engrave.pattern}
            onChange={(kind) =>
              setState((current) => ({
                ...current,
                pattern: patternFor(kind, current.pattern === 'crossHatch'),
              }))
            }
            options={FILLS}
            value={state.pattern === 'offset' ? 'offset' : 'hatch'}
          />
          <NumberField
            disabled={!fill || state.pattern === 'offset'}
            error={errorFor('angle')}
            label={text.engrave.angle}
            onChange={(v) => set('angle', v)}
            unit={strings.units.degrees}
            value={state.angle}
          />
          <CheckField
            checked={state.pattern === 'crossHatch'}
            disabled={!fill || state.pattern === 'offset'}
            label={text.engrave.crossHatch}
            onChange={(cross) =>
              setState((current) => ({ ...current, pattern: patternFor('hatch', cross) }))
            }
          />
        </fieldset>
      </HelperRow>
      <fieldset className="quick-engrave__group" aria-label={text.engrave.noseCone}>
        <CheckField
          checked={state.noseCone}
          label={text.engrave.noseCone}
          onChange={(noseCone) => setState((current) => ({ ...current, noseCone }))}
        />
        <NumberField
          disabled={!state.noseCone}
          error={errorFor('toolDepth')}
          label={text.engrave.toolDepth}
          onChange={(v) => set('toolDepth', v)}
          unit={unit}
          value={state.toolDepth}
        />
        <NumberField
          disabled={!state.noseCone}
          error={errorFor('passes')}
          label={text.engrave.passes}
          onChange={(v) => set('passes', v)}
          step={1}
          value={state.passes}
        />
      </fieldset>
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="quick-engrave__name" htmlFor={nameId}>
        {text.name}
        <input
          className="quick-engrave__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.engrave.title}
          type="text"
          value={state.name}
        />
      </label>
      <ImmediateOutput toolpathId={lastId} />
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
