import { useState, type FormEvent, type JSX } from 'react';

import type { TrimSide } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { pickedComponentIds, useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { canTrim, initialTrimObjectsState } from './model';

import './TrimObjectsForm.scss';

const text = strings.editObjects.trimObjects;

const SIDES: ReadonlyArray<Choice<TrimSide>> = [
  { value: 'inside', label: text.inside },
  { value: 'outside', label: text.outside },
];

export function TrimObjectsForm(): JSX.Element {
  const vectorCount = useDocumentStore((store) => store.selection.ids.length);
  const trimObjectsSelection = useDocumentStore((store) => store.trimObjectsSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState(initialTrimObjectsState);
  const ready = canTrim(vectorCount, pickedComponentIds().length);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (ready) {
      void trimObjectsSelection(state.side, state.refit);
    }
  };

  return (
    <form className="trim-objects-form" onSubmit={onSubmit}>
      <p className="trim-objects-form__hint">{text.hint}</p>
      {ready ? null : <p className="trim-objects-form__notice">{text.needs}</p>}
      <ChoiceField
        label={text.options}
        onChange={(side) => setState((current) => ({ ...current, side }))}
        options={SIDES}
        value={state.side}
      />
      <CheckField
        checked={state.refit}
        label={text.refit}
        onChange={(refit) => setState((current) => ({ ...current, refit }))}
      />
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={ready ? undefined : text.needs}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
