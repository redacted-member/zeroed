import type { JSX } from 'react';

import type { ClearanceOptions } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import type { ClearanceErrors, ClearanceState } from '../../lib/clearanceFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import { DIRECTIONS } from '../ProfileToolpathForm/ProfileToolpathForm';

import './ClearanceOptionsFields.scss';

const text = strings.toolpaths;

const STRATEGIES: ReadonlyArray<Choice<ClearanceOptions['strategy']>> = [
  { value: 'offset', label: text.pocket.offset },
  { value: 'raster', label: text.pocket.raster },
];

interface ClearanceOptionsFieldsProps {
  state: ClearanceState;
  onChange: (state: ClearanceState) => void;
  errors: ClearanceErrors;
  units: Units;
  disabled?: boolean;
  /** Strategy and raster angle belong to the first clearance tool only. */
  strategyDisabled?: boolean;
}

/** How a flat area clearance tool clears: strategy, angle, direction, ramps. */
export function ClearanceOptionsFields({
  state,
  onChange,
  errors,
  units,
  disabled,
  strategyDisabled = false,
}: ClearanceOptionsFieldsProps): JSX.Element {
  return (
    <div className="clearance-options">
      <ChoiceField
        disabled={disabled === true || strategyDisabled}
        label={text.pocket.clearing}
        onChange={(strategy) => onChange({ ...state, strategy })}
        options={STRATEGIES}
        value={state.strategy}
      />
      <NumberField
        disabled={disabled === true || strategyDisabled || state.strategy !== 'raster'}
        error={errors.rasterAngle}
        label={text.pocket.rasterAngle}
        onChange={(rasterAngle) => onChange({ ...state, rasterAngle })}
        unit={strings.units.degrees}
        value={state.rasterAngle}
      />
      <ChoiceField
        disabled={disabled}
        label={text.cutDirection}
        onChange={(direction) => onChange({ ...state, direction })}
        options={DIRECTIONS}
        value={state.direction}
      />
      <CheckField
        checked={state.ramp}
        disabled={disabled}
        label={text.useRamps}
        onChange={(ramp) => onChange({ ...state, ramp })}
      />
      <NumberField
        disabled={disabled === true || !state.ramp}
        error={errors.rampDistance}
        label={text.rampDistance}
        onChange={(rampDistance) => onChange({ ...state, rampDistance })}
        unit={unitSuffix(units)}
        value={state.rampDistance}
      />
    </div>
  );
}
