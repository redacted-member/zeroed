import type { JSX, ReactNode } from 'react';

import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';

import './ButtonFace.scss';

interface ButtonFaceProps {
  /** A shell/ToolIcons id. */
  icon: string;
  /** The button's words, shown small beside the icon; none for an icon only button. */
  label?: ReactNode;
}

/**
 * The inside of a text button: its icon beside its label. The button keeps
 * its own class, handlers and data-tip; this only draws what it shows.
 */
export function ButtonFace({ icon, label }: ButtonFaceProps): JSX.Element {
  // Form and panel icons load after start up; redraw once they are in.
  useExtraIcons();
  return (
    <span className="button-face">
      <span aria-hidden="true" className="button-face__icon">
        {toolIcon(icon)}
      </span>
      {label === undefined ? null : <span className="button-face__label">{label}</span>}
    </span>
  );
}
