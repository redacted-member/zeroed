import { useId, useRef, useState, type JSX, type KeyboardEvent, type MouseEvent } from 'react';

import { currentVariables, evaluate } from '../../lib/expression';
import { insertVariable } from '../VariableMenu/model';
import { VariableMenu } from '../VariableMenu/VariableMenu';

import './NumberField.scss';

interface NumberFieldProps {
  label: string;
  value: string;
  onChange: (text: string) => void;
  /** Suffix such as "mm" or "deg". */
  unit?: string;
  error?: string | undefined;
  disabled?: boolean;
  /** Called on Enter inside the field, typically to create the shape. */
  onEnter?: () => void;
  /** Called when the focus leaves the field (Tab, a click elsewhere). */
  onLeave?: () => void;
  step?: number;
  /** Shown while the field is empty, such as "auto". */
  placeholder?: string;
}

/** A labelled numeric text field with a unit suffix and an error line. */
export function NumberField({
  label,
  value,
  onChange,
  unit,
  error,
  disabled,
  onEnter,
  onLeave,
  step,
  placeholder,
}: NumberFieldProps): JSX.Element {
  const id = useId();
  const errorId = `${id}-error`;
  const inputRef = useRef<HTMLInputElement | null>(null);
  // The right click menu of document variables, at the pointer.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  const onContextMenu = (event: MouseEvent<HTMLInputElement>) => {
    if (disabled === true) {
      return;
    }
    event.preventDefault();
    setMenu({ x: event.clientX, y: event.clientY });
  };

  const insert = (name: string) => {
    const input = inputRef.current;
    const next = insertVariable(
      value,
      name,
      input?.selectionStart ?? null,
      input?.selectionEnd ?? null,
    );
    onChange(next.text);
    window.requestAnimationFrame(() => {
      input?.focus();
      input?.setSelectionRange(next.caret, next.caret);
    });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // A calculation: = puts its result in the box.
    if (event.key === '=' && value.trim() !== '') {
      const result = evaluate(`${value}=`, currentVariables());
      if (result !== null) {
        event.preventDefault();
        onChange(String(Math.round(result * 1e6) / 1e6));
        return;
      }
    }
    if (event.key === 'Enter' && onEnter !== undefined) {
      event.preventDefault();
      onEnter();
      return;
    }
    if (step !== undefined && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      const current = Number(value);
      if (Number.isFinite(current)) {
        event.preventDefault();
        const factor = event.shiftKey ? 10 : 1;
        const next = current + (event.key === 'ArrowUp' ? step : -step) * factor;
        onChange(String(Math.round(next * 1e6) / 1e6));
      }
    }
  };

  return (
    <div className="number-field">
      <label className="number-field__label" htmlFor={id}>
        {label}
      </label>
      <span className="number-field__input">
        <input
          aria-describedby={error === undefined ? undefined : errorId}
          aria-invalid={error !== undefined}
          className="number-field__field"
          disabled={disabled}
          id={id}
          inputMode="decimal"
          onBlur={onLeave}
          onChange={(event) => onChange(event.target.value)}
          onContextMenu={onContextMenu}
          onKeyDown={onKeyDown}
          placeholder={placeholder}
          ref={inputRef}
          type="text"
          value={value}
        />
        {unit === undefined ? null : <span className="number-field__unit">{unit}</span>}
      </span>
      {error === undefined ? null : (
        <p className="number-field__error" id={errorId}>
          {error}
        </p>
      )}
      {menu === null ? null : (
        <VariableMenu onClose={() => setMenu(null)} onInsert={insert} x={menu.x} y={menu.y} />
      )}
    </div>
  );
}
