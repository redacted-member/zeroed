import { useMemo, type FormEvent, type JSX } from 'react';

import type { FitKind } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertFitCurvesUnits,
  fitTolerance,
  fitOptions,
  fitToleranceError,
  initialFitCurvesState,
  maxAngleError,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './FitCurvesForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { fitPicture } from '../HelperPicture/pictures/drawing';

const text = strings.editObjects.fitCurves;

const KINDS: ReadonlyArray<Choice<FitKind>> = [
  { value: 'arcs', label: text.arcs },
  { value: 'beziers', label: text.beziers },
  { value: 'lines', label: text.lines },
];

export function FitCurvesForm(): JSX.Element {
  const selectedCount = useDocumentStore((store) => store.selection.ids.length);
  const fitCurvesSelection = useDocumentStore((store) => store.fitCurvesSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialFitCurvesState, convertFitCurvesUnits);
  const error = useMemo(() => fitToleranceError(state, units), [state, units]);
  const tolerance = fitTolerance(state, units);
  const options = fitOptions(state);
  const canFit = selectedCount >= 1 && tolerance !== null && options !== null;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (canFit) {
      void fitCurvesSelection(tolerance, state.kind, options);
    }
  };

  return (
    <form className="fit-curves-form" onSubmit={onSubmit}>
      <p className="fit-curves-form__hint">{text.hint}</p>
      {selectedCount === 0 ? (
        <p className="fit-curves-form__notice">{strings.editObjects.needsSelection}</p>
      ) : null}
      <fieldset className="fit-curves-form__group">
        <legend className="fit-curves-form__legend">{text.title}</legend>
        <HelperRow picture={<HelperPicture picture={fitPicture(state.kind)} />}>
          <ChoiceField
            label={text.type}
            onChange={(kind) => setState((current) => ({ ...current, kind }))}
            options={KINDS}
            value={state.kind}
          />
        </HelperRow>
        <NumberField
          error={error}
          label={text.tolerance}
          onChange={(value) => setState((current) => ({ ...current, tolerance: value }))}
          unit={unitSuffix(units)}
          value={state.tolerance}
        />
        <CheckField
          checked={state.keepSharp}
          label={text.keepSharp}
          onChange={(keepSharp) => setState((current) => ({ ...current, keepSharp }))}
        />
        {state.keepSharp ? (
          <NumberField
            error={maxAngleError(state)}
            label={text.maxAngle}
            onChange={(maxAngle) => setState((current) => ({ ...current, maxAngle }))}
            unit={text.degrees}
            value={state.maxAngle}
          />
        ) : null}
        <CheckField
          checked={state.replace}
          label={text.replace}
          onChange={(replace) => setState((current) => ({ ...current, replace }))}
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [selectedCount === 0, strings.editObjects.needsSelection],
          [!canFit, strings.why.fixValues],
        )}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
