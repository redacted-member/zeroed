import type { JSX } from 'react';

import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import type { DepthErrors, DepthState } from '../../lib/toolpathFields';
import { unitSuffix } from '../../lib/units';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import type { Picture } from '../HelperPicture/kit';
import { cuttingDepths } from '../HelperPicture/pictures/toolpaths';
import { HelperRow } from '../HelperRow/HelperRow';
import { NumberField } from '../NumberField/NumberField';

import './DepthFields.scss';

interface DepthFieldsProps {
  state: DepthState;
  onChange: (state: DepthState) => void;
  errors: DepthErrors;
  units: Units;
  /** The picture at the left of the fields; Cutting Depths by default. */
  picture?: Picture;
}

/** Cutting Depths: Start Depth and the Cut Depth measured from it. */
export function DepthFields({
  state,
  onChange,
  errors,
  units,
  picture = cuttingDepths,
}: DepthFieldsProps): JSX.Element {
  const text = strings.toolpaths;
  const unit = unitSuffix(units);
  return (
    <fieldset className="depth-fields">
      <legend className="depth-fields__legend">{text.depths}</legend>
      <HelperRow picture={<HelperPicture picture={picture} />}>
        <NumberField
          error={errors.startDepth}
          label={text.startDepth}
          onChange={(startDepth) => onChange({ ...state, startDepth })}
          unit={unit}
          value={state.startDepth}
        />
        <NumberField
          error={errors.cutDepth}
          label={text.cutDepth}
          onChange={(cutDepth) => onChange({ ...state, cutDepth })}
          unit={unit}
          value={state.cutDepth}
        />
      </HelperRow>
    </fieldset>
  );
}
