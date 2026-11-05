import { useId, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { ToolKind } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { insertVariable } from '../ToolDatabase/model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ToolNameFormat.scss';

const text = strings.toolDatabase;

interface ToolNameFormatProps {
  kind: ToolKind;
  /** The type's current default format. */
  format: string;
  variables: readonly string[];
  /** Restore: the tool's name goes back to the format. */
  onRestore: (format: string) => void;
  /** After Set as Default was saved. */
  onSaved: () => void;
}

/** The name format of a tool type: edit it, make it the default, restore a tool to it. */
export function ToolNameFormat(props: ToolNameFormatProps): JSX.Element {
  const { kind, format, variables, onRestore, onSaved } = props;
  const [value, setValue] = useState(format);
  const defaultWhy = value.trim() === '' ? strings.why.typeFormat : undefined;
  const [updateExisting, setUpdateExisting] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const ids = { format: useId(), insert: useId(), update: useId(), title: useId() };

  const setDefault = async () => {
    try {
      const reply = await api.tools.setNameFormat(kind, value, updateExisting);
      setStatus(text.formatSaved(reply.renamed.length));
      onSaved();
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <section aria-labelledby={ids.title} className="tool-name-format">
      <h3 className="tool-name-format__title" id={ids.title}>
        {text.nameFormatTitle(text.kinds[kind])}
      </h3>
      <label className="tool-name-format__label" htmlFor={ids.format}>
        {text.formatField}
      </label>
      <input
        className="tool-name-format__input"
        id={ids.format}
        onChange={(e) => setValue(e.target.value)}
        value={value}
      />
      <label className="tool-name-format__hidden" htmlFor={ids.insert}>
        {text.insertVariable}
      </label>
      <select
        className="tool-name-format__input"
        id={ids.insert}
        onChange={(e) => {
          const variable = e.target.value;
          if (variable !== '') {
            setValue((current) => insertVariable(current, variable));
          }
        }}
        value=""
      >
        <option value="">{text.insertVariableNone}</option>
        {variables.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
      <div className="tool-name-format__check">
        <input
          checked={updateExisting}
          id={ids.update}
          onChange={(e) => setUpdateExisting(e.target.checked)}
          type="checkbox"
        />
        <label htmlFor={ids.update}>{text.updateExisting}</label>
      </div>
      <div className="tool-name-format__actions">
        <button
          className="tool-name-format__button"
          onClick={() => onRestore(value)}
          data-tip={text.restoreHint}
          type="button"
        >
          <ButtonFace icon="formReset" label={text.restoreName} />
        </button>
        <button
          className="tool-name-format__button"
          disabled={defaultWhy !== undefined}
          onClick={() => void setDefault()}
          type="button"
          data-tip={whyTip(text.setDefault, defaultWhy)}
        >
          <ButtonFace icon="setValue" label={text.setDefault} />
        </button>
      </div>
      {status === null ? null : (
        <p aria-live="polite" className="tool-name-format__status">
          {status}
        </p>
      )}
    </section>
  );
}
