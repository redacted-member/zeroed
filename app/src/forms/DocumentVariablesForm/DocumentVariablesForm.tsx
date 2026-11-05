import { useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { strings } from '../../lib/formStrings';
import { useUiStore } from '../../state/uiStore';
import { useVariablesStore } from '../../state/variablesStore';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  formulaResult,
  newRow,
  newVariableError,
  rowValues,
  rowsFrom,
  validateRows,
  variablesFrom,
  type VariableRow,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DocumentVariablesForm.scss';

const text = strings.documentVariables;
const FILTERS = [{ name: text.textFiles, extensions: ['txt'] }];

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Document Variables: a New Variable section above the table of the
 * variables, each change saved as it is made; Export and Import a text file. */
export function DocumentVariablesForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const variables = useVariablesStore((store) => store.variables);
  const formulas = useVariablesStore((store) => store.formulas);
  const [rows, setRows] = useState<VariableRow[]>(() => rowsFrom(variables, formulas));
  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  const [tried, setTried] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const errors = useMemo(() => validateRows(rows), [rows]);
  const values = useMemo(() => rowValues(rows), [rows]);
  const addError = newVariableError(rows, name, value);
  const nameId = useId();
  const valueId = useId();

  const reload = () =>
    useVariablesStore
      .getState()
      .load()
      .then(() => {
        const state = useVariablesStore.getState();
        setRows(rowsFrom(state.variables, state.formulas));
      });

  useEffect(() => {
    void reload().catch((error: unknown) => setFailure(message(error)));
  }, []);

  /** Saves the rows when every one reads. */
  const save = (next: VariableRow[]) => {
    setRows(next);
    const values = variablesFrom(next);
    if (values === null) {
      return;
    }
    setFailure(null);
    useVariablesStore
      .getState()
      .save(values)
      .then(() => setStatus(text.saved))
      .catch((error: unknown) => setFailure(message(error)));
  };

  const add = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setTried(true);
    if (addError !== null) {
      return;
    }
    save([...rows, newRow(name.trim(), value)]);
    setName('');
    setValue('');
    setTried(false);
  };

  const change = (key: number, patch: Partial<VariableRow>) =>
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  const exportFile = async () => {
    const path = await getKernel().chooseSavePath(text.exportTitle, FILTERS, 'variables.txt');
    if (path === null) {
      return;
    }
    try {
      const reply = await api.doc.exportVariables(path);
      setStatus(text.exported(reply.count));
    } catch (error) {
      setFailure(message(error));
    }
  };

  const importFile = async () => {
    const path = await getKernel().chooseOpenPath(text.importTitle, FILTERS);
    if (path === null) {
      return;
    }
    try {
      const reply = await api.doc.importVariables(path);
      await reload();
      setStatus(text.imported(reply.count));
    } catch (error) {
      setFailure(message(error));
    }
  };

  return (
    <div className="document-variables">
      <p className="document-variables__hint">{text.hint}</p>
      <form className="document-variables__group" onSubmit={add}>
        <fieldset className="document-variables__fieldset">
          <legend className="document-variables__legend">{text.newVariable}</legend>
          <label className="document-variables__label" htmlFor={nameId}>
            {text.name}
          </label>
          <input
            aria-invalid={tried && addError !== null}
            className="document-variables__input"
            id={nameId}
            onChange={(event) => setName(event.target.value)}
            type="text"
            value={name}
          />
          <label className="document-variables__label" htmlFor={valueId}>
            {text.value}
          </label>
          <input
            className="document-variables__input"
            id={valueId}
            onChange={(event) => setValue(event.target.value)}
            type="text"
            value={value}
          />
          {tried && addError !== null ? (
            <p className="document-variables__error">{addError}</p>
          ) : null}
          <button className="document-variables__button" type="submit" data-tip={text.add}>
            <ButtonFace icon="formCreate" label={text.add} />
          </button>
        </fieldset>
      </form>
      <section className="document-variables__group" aria-label={text.variables}>
        {rows.length === 0 ? (
          <p className="document-variables__hint">{text.empty}</p>
        ) : (
          <table className="document-variables__table">
            <thead>
              <tr>
                <th scope="col">{text.name}</th>
                <th scope="col">{text.value}</th>
                <th scope="col">
                  <span className="document-variables__hidden">{text.removeShort}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const rowErrors = errors.get(row.key);
                return (
                  <tr key={row.key}>
                    <td>
                      <input
                        aria-invalid={rowErrors?.name !== undefined}
                        aria-label={text.name}
                        className="document-variables__input"
                        onBlur={() => save(rows)}
                        onChange={(event) => change(row.key, { name: event.target.value })}
                        data-tip={rowErrors?.name}
                        type="text"
                        value={row.name}
                      />
                    </td>
                    <td>
                      <input
                        aria-invalid={rowErrors?.value !== undefined}
                        aria-label={text.value}
                        className="document-variables__input"
                        onBlur={() => save(rows)}
                        onChange={(event) => change(row.key, { value: event.target.value })}
                        data-tip={rowErrors?.value ?? formulaResult(row, values)}
                        type="text"
                        value={row.value}
                      />
                    </td>
                    <td>
                      <button
                        aria-label={text.remove(row.name)}
                        className="document-variables__button"
                        onClick={() => save(rows.filter((r) => r.key !== row.key))}
                        type="button"
                        data-tip={text.removeShort}
                      >
                        <ButtonFace icon="itemRemove" label={text.removeShort} />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        <div className="document-variables__buttons">
          <button
            className="document-variables__button"
            onClick={() => void exportFile()}
            type="button"
            data-tip={text.exportFile}
          >
            <ButtonFace icon="exportFile" label={text.exportFile} />
          </button>
          <button
            className="document-variables__button"
            onClick={() => void importFile()}
            type="button"
            data-tip={text.importFile}
          >
            <ButtonFace icon="importModel" label={text.importFile} />
          </button>
        </div>
      </section>
      {status !== null ? (
        <p className="document-variables__hint" role="status">
          {status}
        </p>
      ) : null}
      {failure !== null ? (
        <p className="document-variables__error" role="alert">
          {failure}
        </p>
      ) : null}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
