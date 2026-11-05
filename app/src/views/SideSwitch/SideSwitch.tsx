import type { JSX } from 'react';

import { isDoubleSided, sideLabel, SIDES } from '../../lib/sides';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './SideSwitch.scss';

/** Top and Bottom buttons plus Swap Sides, shown on a double sided job only. */
export function SideSwitch(): JSX.Element | null {
  const job = useDocumentStore((state) => state.job);
  const activeSide = useDocumentStore((state) => state.activeSide);
  const setSide = useDocumentStore((state) => state.setSide);
  const swapSides = useDocumentStore((state) => state.swapSides);

  if (!isDoubleSided(job)) {
    return null;
  }
  return (
    <div aria-label={strings.sides.label} className="side-switch" role="group">
      {SIDES.map((side) => (
        <button
          aria-pressed={side === activeSide}
          className="side-switch__side"
          key={side}
          onClick={() => side !== activeSide && void setSide(side)}
          data-tip={side === 'top' ? strings.sides.topHint : strings.sides.bottomHint}
          type="button"
        >
          {sideLabel(side)}
        </button>
      ))}
      <button
        className="side-switch__swap"
        onClick={() => void swapSides()}
        data-tip={strings.sides.swapHint}
        type="button"
      >
        <ButtonFace icon="swap" label={strings.sides.swap} />
      </button>
    </div>
  );
}
