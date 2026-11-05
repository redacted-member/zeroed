import { useId, useState, type FormEvent, type JSX, type KeyboardEvent } from 'react';

import { strings } from '../../lib/formStrings';
import { definitionsOf, useVariablesStore } from '../../state/variablesStore';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import { definitionOf, newVariableError, rowsFrom } from '../DocumentVariablesForm/model';
import { variableMenu, type VariableAction } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './VariableMenu.scss';

const text = strings.documentVariables;

interface VariableMenuProps {
  x: number;
  y: number;
  /** Called with the name to insert in the field. */
  onInsert: (name: string) => void;
  onClose: () => void;
}

/** The right click menu of a numeric field: insert a document variable, or
 * make a new one and insert it. */
export function VariableMenu({ x, y, onInsert, onClose }: VariableMenuProps): JSX.Element {
  const variables = useVariablesStore((store) => store.variables);
  const formulas = useVariablesStore((store) => store.formulas);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  const [tried, setTried] = useState(false);
  const nameId = useId();
  const valueId = useId();
  const error = newVariableError(rowsFrom(variables, formulas), name, value);

  if (!creating) {
    return (
      <PopupMenu
        entries={variableMenu(Object.keys(variables))}
        label={text.menuLabel}
        onClose={onClose}
        onSelect={(action: VariableAction) => {
          if ('create' in action) {
            setCreating(true);
          } else if (action.insert !== '') {
            onInsert(action.insert);
          }
        }}
        x={x}
        y={y}
      />
    );
  }

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setTried(true);
    if (error !== null) {
      return;
    }
    const trimmed = name.trim();
    void useVariablesStore
      .getState()
      .save({ ...definitionsOf(variables, formulas), [trimmed]: definitionOf(value) })
      .then(() => {
        onInsert(trimmed);
        onClose();
      });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      event.stopPropagation();
      onClose();
    }
  };

  return (
    <form aria-label={text.menuNew} className="variable-menu" onSubmit={submit} role="dialog">
      <label className="variable-menu__label" htmlFor={nameId}>
        {text.name}
      </label>
      <input
        aria-invalid={tried && error !== null}
        className="variable-menu__input"
        id={nameId}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={onKeyDown}
        type="text"
        value={name}
      />
      <label className="variable-menu__label" htmlFor={valueId}>
        {text.value}
      </label>
      <input
        className="variable-menu__input"
        id={valueId}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={onKeyDown}
        type="text"
        value={value}
      />
      {tried && error !== null ? <p className="variable-menu__error">{error}</p> : null}
      <div className="variable-menu__buttons">
        <button className="variable-menu__button" type="submit" data-tip={text.ok}>
          <ButtonFace icon="formApply" label={text.ok} />
        </button>
        <button
          className="variable-menu__button"
          onClick={onClose}
          type="button"
          data-tip={text.cancel}
        >
          <ButtonFace icon="dismiss" label={text.cancel} />
        </button>
      </div>
    </form>
  );
}
