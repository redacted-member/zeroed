import { useMemo, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { passesCrossingCheck } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertOffsetUnits,
  distanceError,
  initialOffsetState,
  offsetArgs,
  type OffsetSide,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './OffsetForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { offsetPicture } from '../HelperPicture/pictures/drawing';

const text = strings.editObjects.offset;

const SIDES: ReadonlyArray<Choice<OffsetSide>> = [
  { value: 'outward', label: text.outward },
  { value: 'inward', label: text.inward },
];

export function OffsetForm(): JSX.Element {
  const selectedCount = useDocumentStore((store) => store.selection.ids.length);
  const offsetSelection = useDocumentStore((store) => store.offsetSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialOffsetState, convertOffsetUnits);
  const error = useMemo(() => distanceError(state, units), [state, units]);
  const args = offsetArgs(state, units);
  const canOffset = selectedCount >= 1 && args !== null;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (canOffset) {
      const checks = usePrefsStore.getState().prefs.confirmations;
      const setting =
        args.corner === 'sharp'
          ? checks.sharpOffsetIntersections
          : checks.radiusedOffsetIntersections;
      const ids = useDocumentStore.getState().selection.ids;
      void passesCrossingCheck(ids, setting, 'offset').then((go) => {
        if (go) {
          void offsetSelection(args);
        }
      });
    }
  };

  return (
    <form className="offset-form" onSubmit={onSubmit}>
      <p className="offset-form__hint">{text.hint}</p>
      {selectedCount === 0 ? (
        <p className="offset-form__notice">{strings.editObjects.needsSelection}</p>
      ) : null}
      <fieldset className="offset-form__group">
        <legend className="offset-form__legend">{text.title}</legend>
        <HelperRow picture={<HelperPicture picture={offsetPicture(state.side, state.corner)} />}>
          <ChoiceField
            label={text.direction}
            onChange={(side) => setState((current) => ({ ...current, side }))}
            options={SIDES}
            value={state.side}
          />
        </HelperRow>
        <NumberField
          error={error}
          label={text.distance}
          onChange={(distance) => setState((current) => ({ ...current, distance }))}
          unit={unitSuffix(units)}
          value={state.distance}
        />
      </fieldset>
      <fieldset className="offset-form__group">
        <legend className="offset-form__legend">{text.options}</legend>
        <CheckField
          checked={state.corner === 'sharp'}
          label={text.sharp}
          onChange={(sharp) =>
            setState((current) => ({ ...current, corner: sharp ? 'sharp' : 'round' }))
          }
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [selectedCount === 0, strings.editObjects.needsSelection],
          [!canOffset, strings.why.fixValues],
        )}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
