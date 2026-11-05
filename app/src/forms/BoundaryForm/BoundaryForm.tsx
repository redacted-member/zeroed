import { useMemo, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  boundaryOffset,
  boundaryOffsetError,
  convertBoundaryUnits,
  initialBoundaryState,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './BoundaryForm.scss';

const text = strings.editObjects.boundary;

export function BoundaryForm(): JSX.Element {
  const selectedCount = useDocumentStore((store) => store.selection.ids.length);
  const boundarySelection = useDocumentStore((store) => store.boundarySelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialBoundaryState, convertBoundaryUnits);
  const error = useMemo(() => boundaryOffsetError(state, units), [state, units]);
  const offset = boundaryOffset(state, units);
  const canCreate = selectedCount >= 1 && offset !== null;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (canCreate) {
      void boundarySelection(offset, 'selection', state.rubberBand);
    }
  };

  return (
    <form className="boundary-form" onSubmit={onSubmit}>
      <p className="boundary-form__hint">{text.hint}</p>
      {selectedCount === 0 ? (
        <p className="boundary-form__notice">{strings.editObjects.needsSelection}</p>
      ) : null}
      <fieldset className="boundary-form__group">
        <legend className="boundary-form__legend">{text.title}</legend>
        <CheckField
          checked={state.useOffset}
          label={text.useOffset}
          onChange={(useOffset) => setState((current) => ({ ...current, useOffset }))}
        />
        <NumberField
          disabled={!state.useOffset}
          error={error}
          label={text.offset}
          onChange={(value) => setState((current) => ({ ...current, offset: value }))}
          unit={unitSuffix(units)}
          value={state.offset}
        />
        <CheckField
          checked={state.rubberBand}
          label={text.rubberBand}
          onChange={(rubberBand) => setState((current) => ({ ...current, rubberBand }))}
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [selectedCount === 0, strings.editObjects.needsSelection],
          [!canCreate, strings.why.fixValues],
        )}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
