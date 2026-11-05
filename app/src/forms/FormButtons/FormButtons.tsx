import type { JSX } from 'react';

import { strings } from '../../lib/strings';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { buttonLook } from './model';

import './FormButtons.scss';

interface FormButtonsProps {
  /** Without a primary action the row holds Close only (forms that act at once). */
  primaryLabel?: string;
  onPrimary?: () => void;
  /**
   * What the primary button waits for, in plain words: the button is off
   * while this is given, and its tooltip says it.
   */
  primaryWhy?: string;
  /** Primary acts as a submit button of the enclosing form when true. */
  submit?: boolean;
  secondaryLabel?: string;
  onSecondary?: () => void;
  onClose: () => void;
  /** The close button's name when it discards the form, such as Cancel. */
  closeLabel?: string;
}

/** A button's icon beside its label. */
function Face({ icon, label }: { icon: string; label: string }): JSX.Element {
  return (
    <>
      <span aria-hidden="true" className="form-buttons__icon">
        {toolIcon(icon)}
      </span>
      <span className="form-buttons__label">{label}</span>
    </>
  );
}

/** The action row every left panel form ends with: each button an icon, a label and a tooltip. */
export function FormButtons({
  primaryLabel,
  onPrimary,
  primaryWhy,
  submit,
  secondaryLabel,
  onSecondary,
  onClose,
  closeLabel,
}: FormButtonsProps): JSX.Element {
  // The icons load after start up; redraw once they are in.
  useExtraIcons();
  const close = closeLabel ?? strings.form.close;
  const closeLook = buttonLook(close);
  const secondaryLook = secondaryLabel === undefined ? null : buttonLook(secondaryLabel);
  const primaryLook = primaryLabel === undefined ? null : buttonLook(primaryLabel, primaryWhy);
  return (
    <div className="form-buttons">
      {secondaryLabel === undefined ||
      secondaryLook === null ||
      onSecondary === undefined ? null : (
        <button
          className="form-buttons__button"
          data-tip={secondaryLook.tip}
          onClick={onSecondary}
          type="button"
        >
          <Face icon={secondaryLook.icon} label={secondaryLabel} />
        </button>
      )}
      {primaryLabel === undefined || primaryLook === null ? null : (
        // A disabled button gets no pointer events, so its slot carries the tooltip.
        <span className="form-buttons__slot" data-tip={primaryLook.tip}>
          <button
            className="form-buttons__button form-buttons__button--primary"
            disabled={primaryWhy !== undefined}
            onClick={submit === true ? undefined : onPrimary}
            type={submit === true ? 'submit' : 'button'}
          >
            <Face icon={primaryLook.icon} label={primaryLabel} />
          </button>
        </span>
      )}
      <button
        className="form-buttons__button"
        data-tip={closeLook.tip}
        onClick={onClose}
        type="button"
      >
        <Face icon={closeLook.icon} label={close} />
      </button>
    </div>
  );
}
