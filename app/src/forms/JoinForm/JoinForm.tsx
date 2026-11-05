import { useMemo, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { convertJoinUnits, initialJoinState, joinTolerance, toleranceError } from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './JoinForm.scss';

export function JoinForm(): JSX.Element {
  const selectedCount = useDocumentStore((store) => store.selection.ids.length);
  const joinSelection = useDocumentStore((store) => store.joinSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const smooth = usePrefsStore((store) => store.prefs.general.smoothJoin);
  const text = strings.editObjects.join;
  const { state, setState, units } = useFormState(initialJoinState, convertJoinUnits);
  const error = useMemo(() => toleranceError(state, units), [state, units]);
  const tolerance = joinTolerance(state, units);
  const canJoin = selectedCount >= 1 && tolerance !== null;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (canJoin) {
      void joinSelection(tolerance, smooth);
    }
  };

  return (
    <form className="join-form" onSubmit={onSubmit}>
      <p className="join-form__hint">{text.hint}</p>
      {selectedCount === 0 ? (
        <p className="join-form__notice">{strings.editObjects.needsSelection}</p>
      ) : null}
      <fieldset className="join-form__group">
        <legend className="join-form__legend">{text.title}</legend>
        <NumberField
          error={error}
          label={text.tolerance}
          onChange={(value) => setState({ tolerance: value })}
          unit={unitSuffix(units)}
          value={state.tolerance}
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [selectedCount === 0, strings.editObjects.needsSelection],
          [!canJoin, strings.why.fixValues],
        )}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
