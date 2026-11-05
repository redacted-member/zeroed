import type { JSX } from 'react';

import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';

import './SafeZReadout.scss';

/**
 * Safe Z and Home Position as Material Setup sets them, shown near the end
 * of every toolpath form.
 */
export function SafeZReadout(): JSX.Element {
  const job = useDocumentStore((store) => store.job);
  const text = strings.rightPanel.material;
  const { machine, units } = job;
  return (
    <dl className="safe-z-readout">
      <div className="safe-z-readout__row">
        <dt className="safe-z-readout__label">{text.safeZ}</dt>
        <dd className="safe-z-readout__value">
          {lengthText(machine.safeZ, units)} {unitSuffix(units)}
        </dd>
      </div>
      <div className="safe-z-readout__row">
        <dt className="safe-z-readout__label">{text.home}</dt>
        <dd className="safe-z-readout__value">
          {text.homeValue(
            lengthText(machine.homeX, units),
            lengthText(machine.homeY, units),
            lengthText(machine.homeZ, units),
          )}
        </dd>
      </div>
    </dl>
  );
}
