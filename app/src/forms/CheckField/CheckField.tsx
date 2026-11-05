import type { JSX } from 'react';

import './CheckField.scss';

interface CheckFieldProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  /** Tooltip shown on hover, such as why the setting is off. */
  tip?: string;
}

/** A labelled checkbox for one on or off setting. */
export function CheckField({
  label,
  checked,
  onChange,
  disabled,
  tip,
}: CheckFieldProps): JSX.Element {
  return (
    <label className="check-field" data-tip={tip}>
      <input
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
      {label}
    </label>
  );
}
