import { useState, type FormEvent, type JSX, type KeyboardEvent } from 'react';

import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import type { TypedValues, ValueField } from '../View3D/handleValues';
import { fieldText, readValues } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './HandleValueBox.scss';

const text = strings.model.transform3d;

interface HandleValueBoxProps {
  /** Where the box's top left corner sits over the 3D view, CSS pixels. */
  x: number;
  y: number;
  fields: readonly ValueField[];
  units: Units;
  onApply: (values: TypedValues) => void;
  onClose: () => void;
}

function suffix(field: ValueField, units: Units): string {
  switch (field.unit) {
    case 'length':
      return unitSuffix(units);
    case 'percent':
      return text.percent;
    case 'angle':
      return text.degrees;
  }
}

/** The values of the active transform handle, editable: Enter applies
 * them, Esc closes the box. Remount it (by key) when the values change
 * from outside. */
export function HandleValueBox({
  x,
  y,
  fields,
  units,
  onApply,
  onClose,
}: HandleValueBoxProps): JSX.Element {
  const [texts, setTexts] = useState<Record<string, string>>({});
  const [bad, setBad] = useState(false);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const values = readValues(fields, texts, units);
    setBad(values === null);
    if (values !== null) {
      setTexts({});
      onApply(values);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  };

  return (
    <form
      aria-label={text.values}
      className="handle-value-box"
      onSubmit={submit}
      style={{ left: x, top: y }}
    >
      {fields.map((field) => {
        const id = `handle-value-${field.key}`;
        return (
          <div className="handle-value-box__row" key={field.key}>
            <label className="handle-value-box__label" htmlFor={id}>
              {text.value[field.key]}
            </label>
            <input
              aria-invalid={bad}
              className="handle-value-box__input"
              id={id}
              inputMode="decimal"
              onChange={(event) => setTexts({ ...texts, [field.key]: event.target.value })}
              onKeyDown={onKeyDown}
              type="text"
              value={texts[field.key] ?? fieldText(field, units)}
            />
            <span className="handle-value-box__unit">{suffix(field, units)}</span>
          </div>
        );
      })}
      {bad ? (
        <p className="handle-value-box__error" role="alert">
          {text.badValue}
        </p>
      ) : null}
      <button className="handle-value-box__submit" type="submit" data-tip={strings.form.apply}>
        <ButtonFace icon="formApply" label={strings.form.apply} />
      </button>
    </form>
  );
}
