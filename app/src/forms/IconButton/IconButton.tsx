import type { JSX, MouseEvent } from 'react';

import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { tipText, whyTip } from '../../shell/Tooltip/model';

import './IconButton.scss';

interface IconButtonProps {
  /** A shell/ToolIcons id. */
  icon: string;
  /** The short text under the icon. */
  label: string;
  /** The full name: the tooltip and what a screen reader says. Defaults to the label. */
  title?: string;
  /** A one line description, shown under the name in the tooltip. */
  description?: string;
  /** Why the button is off, shown in its tooltip; the button is off exactly while given. */
  why?: string;
  /** Set for a button that stays pressed while its mode is on. */
  pressed?: boolean;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}

/** A form button that shows a tool icon with a small label under it. */
export function IconButton({
  icon,
  label,
  title,
  description,
  why,
  pressed,
  onClick,
}: IconButtonProps): JSX.Element {
  // Menu, form and panel icons load after start up; redraw once they are in.
  useExtraIcons();
  return (
    <button
      aria-label={title}
      aria-pressed={pressed}
      className="icon-button"
      disabled={why !== undefined}
      onClick={onClick}
      data-tip={whyTip(tipText(title ?? label, undefined, description), why)}
      type="button"
    >
      <span aria-hidden="true" className="icon-button__icon">
        {toolIcon(icon)}
      </span>
      <span className="icon-button__label">{label}</span>
    </button>
  );
}
