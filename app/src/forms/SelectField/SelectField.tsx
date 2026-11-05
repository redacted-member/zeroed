import { useId, type JSX } from 'react';

import type { Choice } from '../ChoiceField/ChoiceField';

import './SelectField.scss';

interface SelectFieldProps<T extends string> {
  label: string;
  value: T;
  options: ReadonlyArray<Choice<T>>;
  onChange: (value: T) => void;
  disabled?: boolean;
}

/** A labelled drop down list for one enumerated setting. */
export function SelectField<T extends string>({
  label,
  value,
  options,
  onChange,
  disabled,
}: SelectFieldProps<T>): JSX.Element {
  const id = useId();
  return (
    <div className="select-field">
      <label className="select-field__label" htmlFor={id}>
        {label}
      </label>
      <select
        className="select-field__select"
        disabled={disabled}
        id={id}
        onChange={(event) => {
          const next = options.find((option) => option.value === event.target.value);
          if (next !== undefined) {
            onChange(next.value);
          }
        }}
        value={value}
      >
        {options.map((option) => (
          <option disabled={option.disabled} key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}
