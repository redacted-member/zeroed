import { useId, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import type { Anchor } from '../../lib/shapes';

import './AnchorPicker.scss';

interface AnchorPickerProps {
  label: string;
  value: Anchor;
  onChange: (anchor: Anchor) => void;
  /** Five positions (center and corners) or all nine. */
  positions?: 'five' | 'nine';
  disabled?: boolean;
}

const GRID: ReadonlyArray<readonly Anchor[]> = [
  ['topLeft', 'top', 'topRight'],
  ['left', 'centre', 'right'],
  ['bottomLeft', 'bottom', 'bottomRight'],
];

const FIVE: ReadonlySet<Anchor> = new Set([
  'topLeft',
  'topRight',
  'centre',
  'bottomLeft',
  'bottomRight',
]);

const LABELS: Record<Anchor, string> = strings.anchorPicker;

/** The 3 x 3 anchor point radio grid used by size and position forms. */
export function AnchorPicker({
  label,
  value,
  onChange,
  positions = 'five',
  disabled = false,
}: AnchorPickerProps): JSX.Element {
  const name = useId();
  return (
    <fieldset className="anchor-picker" disabled={disabled}>
      <legend className="anchor-picker__legend">{label}</legend>
      <div className="anchor-picker__grid">
        {GRID.flat().map((anchor) => {
          const enabled = positions === 'nine' || FIVE.has(anchor);
          return (
            <label
              className={`anchor-picker__cell${enabled ? '' : ' anchor-picker__cell--off'}`}
              key={anchor}
              data-tip={LABELS[anchor]}
            >
              <input
                aria-label={LABELS[anchor]}
                checked={value === anchor}
                className="anchor-picker__radio"
                disabled={!enabled}
                name={name}
                onChange={() => onChange(anchor)}
                type="radio"
                value={anchor}
              />
              <span className="anchor-picker__dot" />
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
