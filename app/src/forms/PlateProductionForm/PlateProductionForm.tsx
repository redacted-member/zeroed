import { useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { PlateSeparator, PlatesData } from '../../bridge/api';
import { getKernel } from '../../bridge/client';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { bboxHeight, bboxWidth } from '../../lib/transformMath';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertPlateUnits,
  initialPlateState,
  plateArgs,
  platesPerSheet,
  setMargin,
  validatePlates,
  withVariables,
  type FieldChoice,
  type PlateLengthField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy } from '../../shell/Tooltip/model';

import './PlateProductionForm.scss';

const text = strings.editObjects.plates;

const MODES: ReadonlyArray<Choice<'sheets' | 'layers'>> = [
  { value: 'sheets', label: text.modeSheets },
  { value: 'layers', label: text.modeLayers },
];

const SEPARATORS: ReadonlyArray<Choice<PlateSeparator>> = [
  { value: 'comma', label: text.comma },
  { value: 'tab', label: text.tab },
  { value: 'semicolon', label: text.semicolon },
  { value: 'space', label: text.space },
];

const DATA_EXTENSIONS = ['csv', 'txt', 'tsv'];

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The source picker and counter settings of one variable. */
function VariableRow(props: {
  name: string;
  choice: FieldChoice;
  columns: readonly string[];
  error: string | undefined;
  onChange: (choice: FieldChoice) => void;
}): JSX.Element {
  const { name, choice, columns, error, onChange } = props;
  const id = useId();
  const counter = choice.kind === 'counter';
  return (
    <li className="plate-form__variable">
      <label className="plate-form__label" htmlFor={id}>
        {text.source(name)}
      </label>
      <select
        className="plate-form__select"
        id={id}
        onChange={(event) => {
          const value = event.target.value;
          onChange(
            value === 'counter'
              ? { kind: 'counter', start: '1', step: '1', digits: '0' }
              : { kind: 'column', column: Number(value) },
          );
        }}
        value={counter ? 'counter' : String(choice.column)}
      >
        {columns.map((column, index) => (
          <option key={`${column}-${String(index)}`} value={String(index)}>
            {column}
          </option>
        ))}
        <option value="counter">{text.counter}</option>
      </select>
      {choice.kind === 'counter' ? (
        <div className="plate-form__counter">
          <NumberField
            error={error}
            label={text.start}
            onChange={(start) => onChange({ ...choice, start })}
            step={1}
            value={choice.start}
          />
          <NumberField
            label={text.step}
            onChange={(step) => onChange({ ...choice, step })}
            step={1}
            value={choice.step}
          />
          <NumberField
            label={text.digits}
            onChange={(digits) => onChange({ ...choice, digits })}
            step={1}
            value={choice.digits}
          />
        </div>
      ) : null}
    </li>
  );
}

export function PlateProductionForm(): JSX.Element {
  const selection = useDocumentStore((store) => store.selection);
  const job = useDocumentStore((store) => store.job);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialPlateState, convertPlateUnits);
  const [variables, setVariables] = useState<string[]>([]);
  const [templateToolpaths, setTemplateToolpaths] = useState(0);
  const [data, setData] = useState<PlatesData | null>(null);
  const [busy, setBusy] = useState(false);
  const [showErrors, setShowErrors] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const ids = selection.ids;
  const bbox = selection.bbox;

  useEffect(() => {
    if (ids.length === 0) {
      return;
    }
    let live = true;
    void api.plates
      .variables(ids)
      .then((reply) => {
        if (live) {
          setVariables(reply.variables);
          setTemplateToolpaths(reply.toolpaths);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [ids]);

  const { path, separator, header } = state;
  useEffect(() => {
    if (path === null) {
      return;
    }
    let live = true;
    void api.plates
      .data(path, separator, header)
      .then((reply) => {
        if (live) {
          setData(reply);
          setFailure(null);
        }
      })
      .catch((error: unknown) => {
        if (live) {
          setFailure(message(error));
        }
      });
    return () => {
      live = false;
    };
  }, [path, separator, header]);

  const shownVariables = useMemo(() => (ids.length === 0 ? [] : variables), [ids, variables]);
  const columns = useMemo(() => (path !== null && data !== null ? data.columns : []), [path, data]);
  const fitted = useMemo(
    () => withVariables(state, shownVariables, columns),
    [state, shownVariables, columns],
  );
  const errors = useMemo(() => validatePlates(fitted, units), [fitted, units]);
  const plate = bbox === null ? null : { width: bboxWidth(bbox), height: bboxHeight(bbox) };
  const perSheet = platesPerSheet(fitted, units, job, plate);
  const suffix = unitSuffix(units);
  const shown = (mm: number) => `${lengthText(mm, units)} ${suffix}`;
  const errorFor = (field: keyof typeof errors) => (showErrors ? errors[field] : undefined);

  const setLength = (field: PlateLengthField, value: string) =>
    setState((current) =>
      field === 'spacingX' || field === 'spacingY'
        ? { ...current, [field]: value }
        : setMargin(current, field, value),
    );
  const setChoice = (name: string, choice: FieldChoice) =>
    setState((current) => ({ ...current, fields: { ...current.fields, [name]: choice } }));

  const choose = async () => {
    const chosen = await getKernel().chooseOpenPath(text.dialogTitle, [
      { name: text.dataFiles, extensions: DATA_EXTENSIONS },
    ]);
    if (chosen !== null) {
      setData(null);
      setState((current) => ({ ...current, path: chosen }));
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = plateArgs(fitted, units, ids, templateToolpaths);
    if (args === null || busy) {
      return;
    }
    setBusy(true);
    setFailure(null);
    setStatus(null);
    api.plates
      .produce(args)
      .then(async (reply) => {
        await useDocumentStore.getState().refresh();
        setStatus(
          reply.toolpaths.length > 0
            ? `${text.result(reply.plates, reply.sheets)} ${text.toolpathsMade(reply.toolpaths.length)}`
            : text.result(reply.plates, reply.sheets),
        );
      })
      .catch((error: unknown) => setFailure(message(error)))
      .finally(() => setBusy(false));
  };

  const marginFields: ReadonlyArray<{ field: 'left' | 'right' | 'top' | 'bottom'; label: string }> =
    fitted.equalMargins
      ? [{ field: 'left', label: text.margins }]
      : [
          { field: 'top', label: text.top },
          { field: 'bottom', label: text.bottom },
          { field: 'left', label: text.left },
          { field: 'right', label: text.right },
        ];

  return (
    <form className="plate-form" onSubmit={onSubmit}>
      <p className="plate-form__hint">{text.hint}</p>
      {ids.length === 0 ? (
        <p className="plate-form__hint">{strings.editObjects.needsSelection}</p>
      ) : null}
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.materialSizeLegend}</legend>
        <p className="plate-form__hint">{text.size(shown(job.width), shown(job.height))}</p>
      </fieldset>
      {plate !== null ? (
        <fieldset className="plate-form__group">
          <legend className="plate-form__legend">{text.plateSizeLegend}</legend>
          <p className="plate-form__hint">{text.size(shown(plate.width), shown(plate.height))}</p>
        </fieldset>
      ) : null}
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.margins}</legend>
        <CheckField
          checked={fitted.equalMargins}
          label={text.equalMargins}
          onChange={(equalMargins) =>
            setState((current) =>
              equalMargins
                ? {
                    ...current,
                    equalMargins,
                    right: current.left,
                    top: current.left,
                    bottom: current.left,
                  }
                : { ...current, equalMargins },
            )
          }
        />
        {marginFields.map(({ field, label }) => (
          <NumberField
            error={errorFor(field)}
            key={field}
            label={label}
            onChange={(value) => setLength(field, value)}
            unit={suffix}
            value={fitted[field]}
          />
        ))}
      </fieldset>
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.spacing}</legend>
        <NumberField
          error={errorFor('spacingX')}
          label={text.spacingX}
          onChange={(value) => setLength('spacingX', value)}
          unit={suffix}
          value={fitted.spacingX}
        />
        <NumberField
          error={errorFor('spacingY')}
          label={text.spacingY}
          onChange={(value) => setLength('spacingY', value)}
          unit={suffix}
          value={fitted.spacingY}
        />
      </fieldset>
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.numberLegend}</legend>
        {perSheet !== null ? (
          <p className="plate-form__hint" role="status">
            {text.perSheet(perSheet)}
          </p>
        ) : null}
        <CheckField
          checked={fitted.ownSheets}
          label={text.ownSheets}
          onChange={(ownSheets) => setState((current) => ({ ...current, ownSheets }))}
        />
      </fieldset>
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.toolpathOptions}</legend>
        <CheckField
          checked={fitted.toolpaths && templateToolpaths > 0}
          disabled={templateToolpaths === 0}
          label={text.createToolpaths}
          onChange={(toolpaths) => setState((current) => ({ ...current, toolpaths }))}
        />
      </fieldset>
      <ChoiceField
        disabled={fitted.ownSheets}
        label={text.mode}
        onChange={(mode) => setState((current) => ({ ...current, mode }))}
        options={MODES}
        value={fitted.mode}
      />
      <p className="plate-form__hint">{text.modeHint}</p>
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.data}</legend>
        {path === null ? (
          <>
            <p className="plate-form__hint">{text.noData}</p>
            <NumberField
              error={errorFor('count')}
              label={text.count}
              onChange={(count) => setState((current) => ({ ...current, count }))}
              step={1}
              value={fitted.count}
            />
          </>
        ) : (
          <>
            <p className="plate-form__hint">{path.split(/[\\/]/).pop() ?? path}</p>
            {data !== null ? (
              <p className="plate-form__hint">{text.rows(data.rows, data.columns.length)}</p>
            ) : null}
          </>
        )}
        <div className="plate-form__buttons">
          <button
            className="plate-form__button"
            onClick={() => void choose()}
            type="button"
            data-tip={text.choose}
          >
            <ButtonFace icon="fileOpen" label={text.choose} />
          </button>
          {path !== null ? (
            <button
              className="plate-form__button"
              onClick={() => setState((current) => ({ ...current, path: null }))}
              type="button"
              data-tip={text.clear}
            >
              <ButtonFace icon="editDelete" label={text.clear} />
            </button>
          ) : null}
        </div>
        <ChoiceField
          disabled={path === null}
          label={text.separator}
          onChange={(value) => setState((current) => ({ ...current, separator: value }))}
          options={SEPARATORS}
          value={fitted.separator}
        />
        <CheckField
          checked={fitted.header}
          label={text.header}
          onChange={(value) => setState((current) => ({ ...current, header: value }))}
        />
      </fieldset>
      <fieldset className="plate-form__group">
        <legend className="plate-form__legend">{text.variables}</legend>
        {shownVariables.length === 0 ? (
          <p className="plate-form__hint">{text.noVariables}</p>
        ) : (
          <ul className="plate-form__variables">
            {shownVariables.map((name) => {
              const choice = fitted.fields[name];
              return choice === undefined ? null : (
                <VariableRow
                  choice={choice}
                  columns={columns}
                  error={showErrors ? errors[`field:${name}`] : undefined}
                  key={name}
                  name={name}
                  onChange={(next) => setChoice(name, next)}
                />
              );
            })}
          </ul>
        )}
      </fieldset>
      {failure !== null ? (
        <p className="plate-form__error" role="alert">
          {failure}
        </p>
      ) : null}
      {status !== null ? (
        <p className="plate-form__hint" role="status">
          {status}
        </p>
      ) : null}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [busy, strings.why.busy],
          [ids.length === 0, strings.editObjects.needsSelection],
        )}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
