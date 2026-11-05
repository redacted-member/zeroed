import type { JSX, ReactNode } from 'react';

import './HelperRow.scss';

interface HelperRowProps {
  /** The HelperPicture explaining the fields. */
  picture: ReactNode;
  children: ReactNode;
  /** Where the picture sits; most forms put it at the left of the fields. */
  side?: 'left' | 'right';
}

/** A helper picture beside the fields it explains. */
export function HelperRow({ picture, children, side = 'left' }: HelperRowProps): JSX.Element {
  return (
    <div className={`helper-row helper-row--${side}`}>
      {side === 'left' ? picture : null}
      <div className="helper-row__fields">{children}</div>
      {side === 'right' ? picture : null}
    </div>
  );
}
