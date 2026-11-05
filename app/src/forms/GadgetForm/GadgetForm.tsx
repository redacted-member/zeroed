import { useCallback, useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { GadgetDescription, GadgetField, GadgetValue } from '../../bridge/gadgets';
import type { Units } from '../../bridge/types';
import { toolGroups, useToolDatabase } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useGadgetStore } from '../../state/gadgetStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertGadgetUnits,
  fieldDisabled,
  gadgetSections,
  gadgetValues,
  initialGadgetState,
  validateGadget,
  type GadgetFormState,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './GadgetForm.scss';

const text = strings.gadgets;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** A toolpath the gadget made, opened with Edit. */
interface Editing {
  id: number;
  name: string;
  values: Record<string, GadgetValue>;
}

/** The toolpath `id` if `gadget` made it, with the values it ran with; null
 * when it is gone or another gadget made it. Selects the vectors it cuts, as
 * any toolpath form does when it edits. */
async function loadEditing(id: number, gadget: string): Promise<Editing | null> {
  const detail = await api.cam.get(id).catch(() => null);
  if (detail?.gadget?.name !== gadget) {
    return null;
  }
  if (detail.request !== null) {
    await useDocumentStore.getState().select(detail.request.ids);
  }
  return { id, name: detail.name, values: detail.gadget.values };
}

/** A gadget's dialog and the toolpath it edits, for one gadget and toolpath. */
interface Loaded {
  key: string;
  description: GadgetDescription;
  editing: Editing | null;
}

/** Loads the chosen gadget's dialog (and the toolpath being edited), then shows it. */
export function GadgetForm(): JSX.Element {
  const name = useGadgetStore((store) => store.current);
  const editingId = useUiStore((store) => store.editingToolpath);
  const closeForm = useUiStore((store) => store.closeForm);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const wanted = `${name ?? ''}:${editingId ?? 'new'}`;

  useEffect(() => {
    let live = true;
    if (name === null) {
      return undefined;
    }
    Promise.all([
      api.gadgets.describe(name),
      editingId === null ? Promise.resolve(null) : loadEditing(editingId, name),
    ])
      .then(([description, editing]) => {
        if (live) {
          setLoaded({ key: `${name}:${editingId ?? 'new'}`, description, editing });
          setFailure(null);
        }
      })
      .catch((error: unknown) => {
        if (live) {
          setLoaded(null);
          setFailure(message(error));
        }
      });
    return () => {
      live = false;
    };
  }, [name, editingId]);

  if (name === null || failure !== null || loaded === null || loaded.key !== wanted) {
    return (
      <div className="gadget-form">
        {failure !== null ? (
          <p className="gadget-form__error" role="alert">
            {failure}
          </p>
        ) : (
          <p className="gadget-form__hint">{name === null ? text.noneChosen : text.loading}</p>
        )}
        <FormButtons
          onClose={closeForm}
          onPrimary={() => undefined}
          primaryWhy={name === null ? text.noneChosen : failure !== null ? failure : text.loading}
          primaryLabel={text.run}
        />
      </div>
    );
  }
  return (
    <GadgetDialog
      description={loaded.description}
      editGone={editingId !== null && loaded.editing === null}
      editing={loaded.editing}
      key={loaded.key}
    />
  );
}

interface FieldProps {
  field: GadgetField;
  state: GadgetFormState;
  units: Units;
  error: string | undefined;
  onChange: (id: string, value: string | boolean) => void;
}

function GadgetFieldControl({ field, state, units, error, onChange }: FieldProps): JSX.Element {
  const inputId = useId();
  const value = state[field.id];
  const disabled = fieldDisabled(field, state);
  switch (field.kind) {
    case 'tool':
      return (
        <GadgetToolControl
          disabled={disabled}
          error={error}
          field={field}
          onChange={onChange}
          value={value}
        />
      );
    case 'number':
    case 'length':
      return (
        <NumberField
          disabled={disabled}
          error={error}
          label={field.label}
          onChange={(next) => onChange(field.id, next)}
          step={field.kind === 'number' ? 1 : undefined}
          unit={field.kind === 'length' ? unitSuffix(units) : undefined}
          value={typeof value === 'string' ? value : ''}
        />
      );
    case 'check':
      return (
        <CheckField
          checked={value === true}
          disabled={disabled}
          label={field.label}
          onChange={(next) => onChange(field.id, next)}
        />
      );
    case 'choice':
      return (
        <ChoiceField
          disabled={disabled}
          label={field.label}
          onChange={(next) => onChange(field.id, next)}
          options={field.options ?? []}
          value={typeof value === 'string' ? value : ''}
        />
      );
    case 'text':
      return (
        <label className="gadget-form__text" htmlFor={inputId}>
          {field.label}
          <input
            className="gadget-form__input"
            disabled={disabled}
            id={inputId}
            onChange={(event) => onChange(field.id, event.target.value)}
            type="text"
            value={typeof value === 'string' ? value : ''}
          />
        </label>
      );
  }
}

interface ToolProps {
  field: GadgetField;
  disabled: boolean;
  value: string | boolean | undefined;
  error: string | undefined;
  onChange: (id: string, value: string) => void;
}

/** A tool from the database, by id. */
function GadgetToolControl({ field, value, error, disabled, onChange }: ToolProps): JSX.Element {
  const database = useToolDatabase();
  const inputId = useId();
  const errorId = useId();
  return (
    <label className="gadget-form__text" htmlFor={inputId}>
      {field.label}
      <select
        aria-describedby={error === undefined ? undefined : errorId}
        aria-invalid={error !== undefined}
        className="gadget-form__input"
        disabled={disabled || database === null}
        id={inputId}
        onChange={(event) => onChange(field.id, event.target.value)}
        data-tip={field.tooltip}
        value={typeof value === 'string' ? value : ''}
      >
        <option value="">{text.chooseTool}</option>
        {toolGroups(database).map(({ group, tools }) => (
          <optgroup key={group.id} label={group.label}>
            {tools.map((tool) => (
              <option key={tool.id} value={String(tool.id)}>
                {tool.displayName ?? tool.name}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
      {error === undefined ? null : (
        <span className="gadget-form__error" id={errorId}>
          {error}
        </span>
      )}
    </label>
  );
}

interface DialogProps {
  description: GadgetDescription;
  /** The toolpath OK updates, or null to make new ones. */
  editing: Editing | null;
  /** True when Edit opened a toolpath that is gone. */
  editGone: boolean;
}

function GadgetDialog({ description, editing, editGone }: DialogProps): JSX.Element {
  const { fields } = description;
  // An edited toolpath's values, else the last run's (the dialogs
  // remember what was last used), else the defaults.
  const start = editing?.values ?? description.lastValues;
  const refresh = useDocumentStore((store) => store.refresh);
  const closeForm = useUiStore((store) => store.closeForm);
  const selectedCount = useDocumentStore((store) => store.selection.ids.length);
  const needsSelection =
    description.needsSelection !== undefined && selectedCount === 0
      ? description.needsSelection
      : null;
  const sections = useMemo(() => gadgetSections(fields), [fields]);
  const initial = useCallback(
    (units: Units) => initialGadgetState(fields, units, start),
    [fields, start],
  );
  const convert = useCallback(
    (state: GadgetFormState, from: Units, to: Units) => convertGadgetUnits(fields, state, from, to),
    [fields],
  );
  const { state, setState, units } = useFormState(initial, convert);
  const [showErrors, setShowErrors] = useState(false);
  const [busy, setBusy] = useState(false);
  const [messages, setMessages] = useState<string[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const errors = useMemo(() => validateGadget(fields, state, units), [fields, state, units]);

  const onChange = (id: string, value: string | boolean) =>
    setState((current) => ({ ...current, [id]: value }));

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const values = gadgetValues(fields, state, units);
    if (values === null || busy || needsSelection !== null) {
      return;
    }
    setBusy(true);
    setFailure(null);
    setMessages([]);
    try {
      const reply = await api.gadgets.run(description.name, values, editing?.id);
      setMessages(reply.messages.length > 0 ? reply.messages : [text.done]);
    } catch (error) {
      setFailure(message(error));
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  return (
    <form className="gadget-form" onSubmit={(event) => void onSubmit(event)}>
      <h3 className="gadget-form__title">{description.title}</h3>
      {description.description === '' ? null : (
        <p className="gadget-form__hint">{description.description}</p>
      )}
      {editing === null ? null : (
        <p className="gadget-form__notice" role="status">
          {text.editing(editing.name)}
        </p>
      )}
      {editGone ? (
        <p className="gadget-form__notice" role="status">
          {text.editGone}
        </p>
      ) : null}
      {needsSelection === null ? null : (
        <p className="gadget-form__notice" role="status">
          {needsSelection}
        </p>
      )}
      {sections.map((section) => (
        <fieldset className="gadget-form__group" key={section.fields[0]?.id ?? ''}>
          <legend className="gadget-form__legend">{section.group ?? text.settings}</legend>
          {section.fields.map((field) => (
            <div className="gadget-form__field" key={field.id} data-tip={field.tooltip}>
              <GadgetFieldControl
                error={showErrors ? errors[field.id] : undefined}
                field={field}
                onChange={onChange}
                state={state}
                units={units}
              />
            </div>
          ))}
        </fieldset>
      ))}
      {messages.length === 0 ? null : (
        <ul aria-label={text.messages} className="gadget-form__messages" role="status">
          {messages.map((line, index) => (
            <li className="gadget-form__message" key={`${index}-${line}`}>
              {line}
            </li>
          ))}
        </ul>
      )}
      {failure === null ? null : (
        <p className="gadget-form__error" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [busy, strings.why.busy],
          [needsSelection !== null, needsSelection ?? ''],
        )}
        primaryLabel={busy ? text.running : text.run}
        submit
      />
    </form>
  );
}
