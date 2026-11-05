import { useId, useState, type JSX } from 'react';

import { reversedPosition, sliderPosition, typedValue } from './model';

import './SliderBoxField.scss';

interface SliderBoxFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  /** The names at the slider's two ends, such as Dense and Sparse. */
  startLabel: string;
  endLabel: string;
  /** Suffix after the box, such as "%". */
  unit?: string;
  disabled?: boolean;
  /** The slider's start is `max` and its end `min`, for a value that gets
   * finer as it gets smaller (Coarse to Fine). */
  reverse?: boolean;
  onChange: (value: number) => void;
}

/** A slider between two named ends with a box to type the value. */
export function SliderBoxField({
  label,
  value,
  min,
  max,
  step,
  startLabel,
  endLabel,
  unit,
  disabled,
  reverse = false,
  onChange,
}: SliderBoxFieldProps): JSX.Element {
  const id = useId();
  const boxId = useId();
  // The text being typed; the value's own text while the box is not in use.
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? String(value);

  return (
    <div className="slider-box">
      <label className="slider-box__label" htmlFor={boxId}>
        {label}
      </label>
      <span className="slider-box__box">
        <input
          aria-invalid={typedValue(shown, min, max) === null}
          className="slider-box__input"
          disabled={disabled}
          id={boxId}
          inputMode="decimal"
          onBlur={() => setDraft(null)}
          onChange={(event) => {
            setDraft(event.target.value);
            const typed = typedValue(event.target.value, min, max);
            if (typed !== null) {
              onChange(typed);
            }
          }}
          type="text"
          value={shown}
        />
        {unit === undefined ? null : <span className="slider-box__unit">{unit}</span>}
      </span>
      <input
        aria-label={label}
        className="slider-box__range"
        disabled={disabled}
        id={id}
        max={max}
        min={min}
        onChange={(event) => {
          setDraft(null);
          const position = Number(event.target.value);
          onChange(reverse ? reversedPosition(position, min, max) : position);
        }}
        step={step}
        type="range"
        value={reverse ? reversedPosition(value, min, max) : sliderPosition(value, min, max)}
      />
      <span aria-hidden="true" className="slider-box__ends">
        <span>{startLabel}</span>
        <span>{endLabel}</span>
      </span>
    </div>
  );
}
