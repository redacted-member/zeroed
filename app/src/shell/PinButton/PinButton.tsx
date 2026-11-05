import type { JSX } from 'react';

import './PinButton.scss';

interface PinButtonProps {
  pinned: boolean;
  /** Tip while pinned (clicking auto hides the panel). */
  pinLabel: string;
  /** Tip while unpinned (clicking keeps the panel open). */
  unpinLabel: string;
  onToggle: () => void;
}

/** The push pin in a dock panel's title area: pinned stays open, unpinned auto hides. */
export function PinButton({ pinned, pinLabel, unpinLabel, onToggle }: PinButtonProps): JSX.Element {
  const label = pinned ? pinLabel : unpinLabel;
  return (
    <button
      aria-label={label}
      aria-pressed={pinned}
      className="pin-button"
      onClick={onToggle}
      data-tip={label}
      type="button"
    >
      <svg aria-hidden="true" className="pin-button__icon" focusable="false" viewBox="0 0 16 16">
        {pinned ? (
          <path d="M5 2h6M6 2v5l-2 3h8l-2-3V2M8 10v4" />
        ) : (
          <path d="M14 5v6M14 6H9L6 4v8l3-2h5M6 8H2" />
        )}
      </svg>
    </button>
  );
}
