import type { JSX } from 'react';

import type { GuideId } from '../../state/guideStore';

import './GuideIcon.scss';

export type GuideIconName = GuideId | 'back' | 'next' | 'close' | 'menu';

const PATHS: Readonly<Record<GuideIconName, string>> = {
  tour: 'M2 3h12v10H2zM2 6h12M6 6v7',
  sign: 'M2 4h12v8H2zM5 7l1.5 3L8 7l1.5 3L11 7',
  relief: 'M1.5 12.5h13M3 12.5c1-5 3-7 5-7s4 2 5 7',
  box: 'M2 5l6-3 6 3v6l-6 3-6-3zM2 5l6 3 6-3M8 8v6',
  back: 'M10 3 5 8l5 5',
  next: 'M6 3l5 5-5 5',
  close: 'M4 4l8 8M12 4l-8 8',
  menu: 'M3 4h10M3 8h10M3 12h10',
};

/** The line icons of the getting started guide's buttons, one per walkthrough and action. */
export function GuideIcon({
  name,
  large = false,
}: {
  name: GuideIconName;
  large?: boolean;
}): JSX.Element {
  return (
    <svg
      aria-hidden="true"
      className={large ? 'guide-icon guide-icon--large' : 'guide-icon'}
      focusable="false"
      viewBox="0 0 16 16"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
