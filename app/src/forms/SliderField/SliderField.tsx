import { useId, type JSX } from 'react';

import './SliderField.scss';

interface SliderFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Suffix after the value, such as "%" or "deg". */
  unit?: string;
  /** Shown instead of the value, such as the length a percentage stands for. */
  readout?: string;
  disabled?: boolean;
  onChange: (value: number) => void;
  /** Called when a drag ends or a key moves the slider, to act on the value. */
  onCommit?: (value: number) => void;
}

/** A labelled range slider showing its value, for amounts a form previews live. */
export function SliderField({
  label,
  value,
  min,
  max,
  step,
  unit,
  readout,
  disabled,
  onChange,
  onCommit,
}: SliderFieldProps): JSX.Element {
  const id = useId();
  const commit = (text: string) => onCommit?.(Number(text));
  return (
    <div className="slider-field">
      <label className="slider-field__label" htmlFor={id}>
        {label}
      </label>
      <output className="slider-field__value" htmlFor={id}>
        {readout ?? (unit === undefined ? value : `${value} ${unit}`)}
      </output>
      <input
        className="slider-field__range"
        disabled={disabled}
        id={id}
        max={max}
        min={min}
        onChange={(event) => onChange(Number(event.target.value))}
        onKeyUp={(event) => commit(event.currentTarget.value)}
        onPointerUp={(event) => commit(event.currentTarget.value)}
        step={step}
        type="range"
        value={value}
      />
    </div>
  );
}
