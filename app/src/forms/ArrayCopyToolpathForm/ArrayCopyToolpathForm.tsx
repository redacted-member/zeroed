import { useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  arrayCopySpec,
  arraySources,
  convertArrayCopyUnits,
  initialArrayCopyState,
  mixesTools,
  validateArrayCopy,
  type ArrayCopyField,
  type ArrayCopyState,
} from './model';

import './ArrayCopyToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { toolpathGrid } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths.arrayCopy;

const SPACINGS: ReadonlyArray<Choice<ArrayCopyState['spacing']>> = [
  { value: 'offset', label: text.offset },
  { value: 'gap', label: text.gap },
];

export function ArrayCopyToolpathForm(): JSX.Element {
  const toolpaths = useToolpathStore((store) => store.toolpaths);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialArrayCopyState, convertArrayCopyUnits);
  const [showErrors, setShowErrors] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const nameId = useId();
  const sources = arraySources(toolpaths);
  const mixed = mixesTools(sources);
  const errors = useMemo(() => validateArrayCopy(state, units, text.gridError), [state, units]);
  const unit = unitSuffix(units);
  const set = (field: ArrayCopyField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: ArrayCopyField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const spec = arrayCopySpec(state, units, sources);
    if (spec === null || sources.length === 0) {
      return;
    }
    void useToolpathStore
      .getState()
      .arrayCopy(
        sources.map((t) => t.id),
        spec,
        state.name.trim(),
      )
      .then((ok) => {
        setFailure(ok ? null : useDocumentStore.getState().lastError);
        if (ok) {
          closeForm();
        }
      });
  };

  return (
    <form className="array-copy" onSubmit={onSubmit}>
      <p className="array-copy__hint">{text.hint}</p>
      <fieldset className="array-copy__group">
        <legend className="array-copy__legend">{text.list}</legend>
        {sources.length === 0 ? (
          <p className="array-copy__notice">{text.none}</p>
        ) : (
          <ul className="array-copy__list">
            {sources.map((t) => (
              <li className="array-copy__item" key={t.id}>
                {t.name}
                <span className="array-copy__tool">{t.toolName}</span>
              </li>
            ))}
          </ul>
        )}
      </fieldset>
      <fieldset className="array-copy__group">
        <legend className="array-copy__legend">{text.grid}</legend>
        <HelperRow picture={<HelperPicture picture={toolpathGrid} />}>
          <NumberField
            error={errorFor('rows')}
            label={text.rows}
            onChange={(v) => set('rows', v)}
            step={1}
            value={state.rows}
          />
          <NumberField
            error={errorFor('columns')}
            label={text.columns}
            onChange={(v) => set('columns', v)}
            step={1}
            value={state.columns}
          />
        </HelperRow>
        {showErrors && errors.grid !== undefined ? (
          <p className="array-copy__notice" role="alert">
            {errors.grid}
          </p>
        ) : null}
      </fieldset>
      <fieldset className="array-copy__group">
        <legend className="array-copy__legend">{text.spacing}</legend>
        <ChoiceField
          label={text.spacing}
          onChange={(spacing) => setState((current) => ({ ...current, spacing }))}
          options={SPACINGS}
          value={state.spacing}
        />
        <NumberField
          error={errorFor('x')}
          label={text.x}
          onChange={(v) => set('x', v)}
          unit={unit}
          value={state.x}
        />
        <NumberField
          error={errorFor('y')}
          label={text.y}
          onChange={(v) => set('y', v)}
          unit={unit}
          value={state.y}
        />
      </fieldset>
      <CheckField
        checked={state.minimize}
        disabled={!mixed}
        label={text.minimize}
        onChange={(minimize) => setState((current) => ({ ...current, minimize }))}
      />
      {failure === null ? null : (
        <p className="array-copy__notice" role="alert">
          {failure}
        </p>
      )}
      <label className="array-copy__name" htmlFor={nameId}>
        {text.name}
        <input
          className="array-copy__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.defaultName}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={sources.length === 0 ? text.none : undefined}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
