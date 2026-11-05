import { useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { usePreview } from '../../lib/useDrawController';
import { useEntityPaths } from '../../lib/useEntityPaths';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { CheckField } from '../CheckField/CheckField';
import { ModelingActions } from '../ModelingActions/ModelingActions';
import { useModelingApply } from '../ModelingActions/useModelingApply';
import { NumberField } from '../NumberField/NumberField';
import {
  convertTurnSpinUnits,
  initialTurnSpinState,
  profileFromSelection,
  revolveArgs,
  revolvePreview,
  validateTurnSpin,
  type RevolveMode,
  type TurnSpinField,
  type TurnSpinState,
} from './model';

import './TurnSpinForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { turnSpinPicture } from '../HelperPicture/pictures/modeling';

const text = strings.model.turnSpin;

const MODES: ReadonlyArray<Choice<RevolveMode>> = [
  { value: 'turn', label: text.turn },
  { value: 'spin', label: text.spin },
];

export function TurnSpinForm(): JSX.Element {
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialTurnSpinState, convertTurnSpinUnits);
  const { apply, reset, startNew } = useModelingApply(null);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateTurnSpin(state, units), [state, units]);
  const profile = profileFromSelection(selectedIds, entities);
  const paths = useEntityPaths(profile === null ? [] : [profile]);
  const profilePath = profile === null ? undefined : paths.get(profile);
  usePreview(useMemo(() => revolvePreview(state, profilePath), [state, profilePath]));
  const unit = unitSuffix(units);

  const set = (patch: Partial<TurnSpinState>) => setState((current) => ({ ...current, ...patch }));
  const errorFor = (field: TurnSpinField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = revolveArgs(state, units);
    if (args === null || profile === null) {
      return;
    }
    const name = state.name.trim();
    const common = { id: profile, combine: state.combine, ...(name === '' ? {} : { name }) };
    void apply((target) =>
      args.mode === 'turn'
        ? api.model.turn({ ...common, params: args.params, ...target })
        : api.model.spin({ ...common, params: args.params, ...target }),
    ).then(() => {
      if (useUiStore.getState().viewMode === '2d') {
        useUiStore.getState().setViewMode('split');
      }
    });
  };

  return (
    <form className="turn-spin" onSubmit={onSubmit}>
      <p className="turn-spin__hint">{text.hint}</p>
      {profile === null ? <p className="turn-spin__notice">{text.needsProfile}</p> : null}
      <fieldset className="turn-spin__group">
        <legend className="turn-spin__legend">{text.mode}</legend>
        <HelperRow picture={<HelperPicture picture={turnSpinPicture(state.mode)} />}>
          <ChoiceField
            label={text.mode}
            onChange={(mode) => set({ mode })}
            options={MODES}
            value={state.mode}
          />
          <p className="turn-spin__hint">{state.mode === 'turn' ? text.turnHint : text.spinHint}</p>
        </HelperRow>
      </fieldset>
      <fieldset className="turn-spin__group">
        <legend className="turn-spin__legend">{text.settings}</legend>
        <CheckField
          checked={state.exactHeight}
          label={text.exactHeight}
          onChange={(exactHeight) => set({ exactHeight })}
        />
        <NumberField
          disabled={!state.exactHeight}
          error={errorFor('height')}
          label={text.height}
          onChange={(height) => set({ height })}
          unit={unit}
          value={state.height}
        />
      </fieldset>
      <ModelingActions
        applyWhy={profile === null ? text.needsProfile : undefined}
        combine={state.combine}
        name={state.name}
        onClose={closeForm}
        onCombine={(combine) => set({ combine })}
        onName={(name) => set({ name })}
        onReset={() => void reset()}
        onStartNew={() => void startNew()}
      />
    </form>
  );
}
