import { useId, useState, type JSX } from 'react';

import type { ToolVariable } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { usesVariable, validVariableName } from '../ToolDatabase/model';
import { withVariable, withoutVariable } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './ToolVariables.scss';

const text = strings.toolDatabase;

interface ToolVariablesProps {
  /** The tool's name, to refuse deleting a variable it still uses. */
  name: string;
  variables: ToolVariable[];
  onChange: (variables: ToolVariable[]) => void;
}

/** A tool's own naming variables: list, create and delete. Saved with Apply. */
export function ToolVariables({ name, variables, onChange }: ToolVariablesProps): JSX.Element {
  const [newName, setNewName] = useState('');
  const [newValue, setNewValue] = useState('');
  const [problem, setProblem] = useState<string | null>(null);
  const ids = { title: useId(), name: useId(), value: useId() };

  const create = () => {
    if (!validVariableName(newName)) {
      setProblem(text.variableBadName);
      return;
    }
    onChange(withVariable(variables, newName, newValue));
    setNewName('');
    setNewValue('');
    setProblem(null);
  };

  const remove = (variable: string) => {
    if (usesVariable(name, variable)) {
      setProblem(text.variableInUse(variable));
      return;
    }
    onChange(withoutVariable(variables, variable));
    setProblem(null);
  };

  return (
    <section aria-labelledby={ids.title} className="tool-variables">
      <h3 className="tool-variables__title" id={ids.title}>
        {text.variablesTitle}
      </h3>
      {variables.length === 0 ? (
        <p className="tool-variables__hint">{text.noVariables}</p>
      ) : (
        <table className="tool-variables__table">
          <thead>
            <tr>
              <th scope="col">{text.variableName}</th>
              <th scope="col">{text.variableValue}</th>
              <td />
            </tr>
          </thead>
          <tbody>
            {variables.map((variable) => (
              <tr key={variable.name}>
                <td>{`{${variable.name}}`}</td>
                <td>{variable.value}</td>
                <td>
                  <button
                    aria-label={text.deleteVariable(variable.name)}
                    className="tool-variables__button"
                    onClick={() => remove(variable.name)}
                    type="button"
                    data-tip={strings.toolDatabase.remove}
                  >
                    <ButtonFace icon="itemRemove" label={strings.toolDatabase.remove} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <fieldset className="tool-variables__new">
        <legend className="tool-variables__legend">{text.newVariable}</legend>
        <label className="tool-variables__label" htmlFor={ids.name}>
          {text.variableName}
        </label>
        <input
          className="tool-variables__input"
          id={ids.name}
          onChange={(e) => setNewName(e.target.value)}
          value={newName}
        />
        <label className="tool-variables__label" htmlFor={ids.value}>
          {text.variableValue}
        </label>
        <input
          className="tool-variables__input"
          id={ids.value}
          onChange={(e) => setNewValue(e.target.value)}
          value={newValue}
        />
        <button
          className="tool-variables__button"
          onClick={create}
          type="button"
          data-tip={text.createVariable}
        >
          <ButtonFace icon="formCreate" label={text.createVariable} />
        </button>
      </fieldset>
      {problem === null ? null : (
        <p className="tool-variables__error" role="alert">
          {problem}
        </p>
      )}
    </section>
  );
}
