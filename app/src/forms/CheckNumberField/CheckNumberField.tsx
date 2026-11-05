import { useId, type JSX } from 'react';

import './CheckNumberField.scss';

interface CheckNumberFieldProps {
  /** Names both the check box and the value it switches on. */
  label: string;
  checked: boolean;
  onCheck: (checked: boolean) => void;
  value: string;
  onChange: (text: string) => void;
  unit?: string;
  error?: string | undefined;
  /** Shown in the field while the check box is off, such as "None". */
  offText?: string;
  disabled?: boolean;
}

/**
 * One row with a check box that switches on an optional value and the
 * value's field beside it, such as a flat depth.
 */
export function CheckNumberField({
  label,
  checked,
  onCheck,
  value,
  onChange,
  unit,
  error,
  offText,
  disabled,
}: CheckNumberFieldProps): JSX.Element {
  const id = useId();
  const errorId = `${id}-error`;
  const off = disabled === true || !checked;
  return (
    <div className="check-number-field">
      <label className="check-number-field__label">
        <input
          checked={checked}
          disabled={disabled}
          onChange={(event) => onCheck(event.target.checked)}
          type="checkbox"
        />
        {label}
      </label>
      <span className="check-number-field__input">
        <input
          aria-describedby={error === undefined ? undefined : errorId}
          aria-invalid={error !== undefined}
          aria-label={label}
          className="check-number-field__field"
          disabled={off}
          id={id}
          inputMode="decimal"
          onChange={(event) => onChange(event.target.value)}
          type="text"
          value={checked || offText === undefined ? value : offText}
        />
        {unit === undefined ? null : <span className="check-number-field__unit">{unit}</span>}
      </span>
      {error === undefined ? null : (
        <p className="check-number-field__error" id={errorId}>
          {error}
        </p>
      )}
    </div>
  );
}
