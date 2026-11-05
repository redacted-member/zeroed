import type { JSX } from 'react';

import type { GuideId } from '../../state/guideStore';

import './GuideIcon.scss';

export type GuideIconName = GuideId | 'back' | 'next' | 'close' | 'menu';

const PATHS: Readonly<Record<GuideIconName, string>> = {
  tour: 'M2 3h12v10H2zM2 6h12M6 6v7',
  sign: 'M2 4h12v8H2zM5 7l1.5 3L8 7l1.5 3L11 7',
  relief: 'M1.5 12.5h13M3 12.5c1-5 3-7 5-7s4 2 5 7',
  box: 'M2 5l6-3 6 3v6l-6 3-6-3zM2 5l6 3 6-3M8 8v6',
  machine: 'M2 13h12M4 13V3h8v10M4 6h8M8 6v4M7 10h2',
  cutout: 'M4 4h8v8H4zM2 2h12v12H2z',
  rotary: 'M3 5h10M3 11h10M3 5a2 3 0 0 0 0 6M13 5a2 3 0 0 1 0 6M13 5a2 3 0 0 0 0 6',
  trace: 'M2 3h12v10H2zM4 11l3-4 2 2 1-1 2 3',
  twoSided: 'M2 6h9v7H2zM5 6V3h9v7h-3',
  nesting: 'M2 2h12v12H2zM2 8h6v6M8 2v6h6',
  furniture: 'M3 2h10v12H3zM3 8h10M7 5h2M7 11h2',
  laser: 'M8 1v7M5 8h6M8 8v4M6 14h4',
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
