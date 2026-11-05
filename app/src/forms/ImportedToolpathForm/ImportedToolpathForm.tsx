import { useEffect, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { ImportedToolpath } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import {
  importedEdit,
  importedState,
  validateImported,
  type ImportedField,
  type ImportedFormState,
} from './model';

import './ImportedToolpathForm.scss';

const text = strings.toolpaths.imported;

/** Edits a toolpath read from a G-code file: name, place, depth and feeds. */
export function ImportedToolpathForm(): JSX.Element {
  const id = useUiStore((store) => store.editingToolpath);
  const closeForm = useUiStore((store) => store.closeForm);
  const units = useDocumentStore((store) => store.job.units);
  const [loaded, setLoaded] = useState<ImportedToolpath | null>(null);
  const [state, setState] = useState<ImportedFormState | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    if (id === null) {
      return;
    }
    let cancelled = false;
    void api.cam.imported
      .get(id)
      .then((toolpath) => {
        if (!cancelled) {
          setLoaded(toolpath);
          setState(importedState(toolpath, useDocumentStore.getState().job.units));
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setFailure(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [id]);

  const errors = useMemo(
    () => (state === null ? {} : validateImported(state, units)),
    [state, units],
  );

  if (id === null || state === null || loaded === null) {
    return (
      <div className="imported-toolpath">
        <p className="imported-toolpath__hint" role={failure === null ? 'status' : 'alert'}>
          {failure ?? text.loading}
        </p>
        <FormButtons onClose={closeForm} onPrimary={closeForm} primaryLabel={strings.form.close} />
      </div>
    );
  }

  const apply = () => {
    const change = importedEdit(id, state, units);
    if (change === null) {
      return;
    }
    void api.cam.imported
      .edit(change)
      .then(async () => {
        setFailure(null);
        await useDocumentStore.getState().refresh();
        await useToolpathStore.getState().refresh();
      })
      .catch((error: unknown) =>
        setFailure(error instanceof Error ? error.message : String(error)),
      );
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    apply();
  };

  const unit = unitSuffix(units);
  const field = (name: ImportedField, label: string, suffix: string) => (
    <NumberField
      error={errors[name]}
      label={label}
      onChange={(value) => setState((current) => current && { ...current, [name]: value })}
      onEnter={apply}
      unit={suffix}
      value={state[name]}
    />
  );

  return (
    <form className="imported-toolpath" onSubmit={onSubmit}>
      <p className="imported-toolpath__hint">{text.hint}</p>
      <p className="imported-toolpath__file">{text.file(loaded.file)}</p>
      <label className="imported-toolpath__name">
        {text.name}
        <input
          aria-invalid={errors.name !== undefined}
          className="imported-toolpath__input"
          onChange={(event) =>
            setState((current) => current && { ...current, name: event.target.value })
          }
          type="text"
          value={state.name}
        />
      </label>
      {errors.name === undefined ? null : (
        <p className="imported-toolpath__notice" role="alert">
          {errors.name}
        </p>
      )}
      <fieldset className="imported-toolpath__group">
        <legend className="imported-toolpath__legend">{text.position}</legend>
        {field('offsetX', text.offsetX, unit)}
        {field('offsetY', text.offsetY, unit)}
        {field('startDepth', text.startDepth, unit)}
      </fieldset>
      <fieldset className="imported-toolpath__group">
        <legend className="imported-toolpath__legend">{text.feeds}</legend>
        {field('feed', text.feed, text.perMinute(unit))}
        {field('plunge', text.plunge, text.perMinute(unit))}
        {field('spindle', text.spindle, text.rpm)}
      </fieldset>
      <fieldset className="imported-toolpath__group">
        <legend className="imported-toolpath__legend">{text.tool}</legend>
        <ToolPicker
          onChange={(tool) => setState((current) => current && { ...current, tool })}
          tool={state.tool}
          units={units}
        />
      </fieldset>
      {failure === null ? null : (
        <p className="imported-toolpath__notice" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={apply}
        primaryWhy={Object.keys(errors).length > 0 ? strings.why.fixValues : undefined}
        primaryLabel={text.apply}
        submit
      />
    </form>
  );
}
