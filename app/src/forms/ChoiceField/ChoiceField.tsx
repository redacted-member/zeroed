import { useId, type JSX } from 'react';

import { HelperPicture } from '../HelperPicture/HelperPicture';
import type { Picture } from '../HelperPicture/kit';

import './ChoiceField.scss';

export interface Choice<T extends string> {
  value: T;
  label: string;
  /** Greys out this one option. */
  disabled?: boolean;
  /** A small picture beside the option. */
  picture?: Picture;
}

interface ChoiceFieldProps<T extends string> {
  label: string;
  value: T;
  options: ReadonlyArray<Choice<T>>;
  onChange: (value: T) => void;
  disabled?: boolean;
}

/** A labelled row of radio buttons for one enumerated setting. */
export function ChoiceField<T extends string>({
  label,
  value,
  options,
  onChange,
  disabled,
}: ChoiceFieldProps<T>): JSX.Element {
  const name = useId();
  return (
    <fieldset className="choice-field" disabled={disabled}>
      <legend className="choice-field__label">{label}</legend>
      <div className="choice-field__options">
        {options.map((option) => (
          <label className="choice-field__option" key={option.value}>
            <input
              checked={value === option.value}
              disabled={option.disabled}
              name={name}
              onChange={() => onChange(option.value)}
              type="radio"
              value={option.value}
            />
            {option.picture === undefined ? null : (
              <HelperPicture picture={option.picture} size="icon" />
            )}
            {option.label}
          </label>
        ))}
      </div>
    </fieldset>
  );
}
