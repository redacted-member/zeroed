import { useMemo, useState, type FormEvent, type JSX } from 'react';

import type { CopyAlongArgs } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertCopyAlongUnits,
  copyAlongArgs,
  copyAlongReady,
  copyAlongRoles,
  initialCopyAlongState,
  validateCopyAlong,
  type CopyAlongField,
  type CopyWhat,
} from './model';

import './CopyAlongForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { copyAlong } from '../HelperPicture/pictures/drawing';

const text = strings.editObjects.copyAlong;

const MODES: ReadonlyArray<Choice<CopyAlongArgs['distributeMode']>> = [
  { value: 'spacing', label: text.bySpacing },
  { value: 'count', label: text.byCount },
];

const WHAT: ReadonlyArray<Choice<CopyWhat>> = [
  { value: 'object', label: text.copyObject },
  { value: 'circles', label: text.copyCircles },
];

export function CopyAlongForm(): JSX.Element {
  const selected = useDocumentStore((store) => store.selection.ids);
  const copyAlongSelection = useDocumentStore((store) => store.copyAlongSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialCopyAlongState, convertCopyAlongUnits);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateCopyAlong(state, units), [state, units]);
  const unit = unitSuffix(units);
  const pickedCount = useModelStore((store) => pickedComponents(store).length);
  const ready = copyAlongReady(selected, state.what, pickedCount);

  const setField = (field: CopyAlongField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: CopyAlongField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const { objects, curves } = copyAlongRoles(selected, state.what, pickedCount);
    const args = copyAlongArgs(state, units, curves);
    if (args !== null && ready) {
      void copyAlongSelection(objects, args);
    }
  };

  return (
    <form className="copy-along-form" onSubmit={onSubmit}>
      <p className="copy-along-form__hint">{text.hint}</p>
      {ready ? null : <p className="copy-along-form__notice">{text.needsPath}</p>}
      <ChoiceField
        label={text.what}
        onChange={(what) => setState((current) => ({ ...current, what }))}
        options={WHAT}
        value={state.what}
      />
      {state.what === 'circles' ? (
        <NumberField
          error={errorFor('diameter')}
          label={text.diameter}
          onChange={(v) => setField('diameter', v)}
          unit={unit}
          value={state.diameter}
        />
      ) : null}
      <fieldset className="copy-along-form__group">
        <legend className="copy-along-form__legend">{text.spacingBy}</legend>
        <HelperRow picture={<HelperPicture picture={copyAlong} />}>
          <ChoiceField
            label={text.spacingBy}
            onChange={(distributeMode) => setState((current) => ({ ...current, distributeMode }))}
            options={MODES}
            value={state.distributeMode}
          />
          {state.distributeMode === 'count' ? (
            <NumberField
              error={errorFor('count')}
              label={text.count}
              onChange={(v) => setField('count', v)}
              step={1}
              value={state.count}
            />
          ) : (
            <>
              <NumberField
                error={errorFor('spacing')}
                label={text.spacing}
                onChange={(v) => setField('spacing', v)}
                unit={unit}
                value={state.spacing}
              />
              <CheckField
                checked={state.forceEven}
                label={text.forceEven}
                onChange={(forceEven) => setState((current) => ({ ...current, forceEven }))}
              />
            </>
          )}
        </HelperRow>
      </fieldset>
      <CheckField
        checked={state.alignToCurve}
        label={text.align}
        onChange={(alignToCurve) => setState((current) => ({ ...current, alignToCurve }))}
      />
      <CheckField
        checked={state.newLayer}
        label={text.newLayer}
        onChange={(newLayer) => setState((current) => ({ ...current, newLayer }))}
      />
      <CheckField
        checked={state.reverse}
        label={text.reverse}
        onChange={(reverse) => setState((current) => ({ ...current, reverse }))}
      />
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={ready ? undefined : text.needsPath}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
