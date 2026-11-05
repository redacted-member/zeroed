import type { JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { usePrefsStore } from '../../state/prefsStore';
import { usageLines, type UsageMode } from './model';

import './UsageInfo.scss';

interface UsageInfoProps {
  mode: UsageMode;
  moveKey: string;
}

/** The line of mouse and key hints along the bottom of the 3D view. */
export function UsageInfo({ mode, moveKey }: UsageInfoProps): JSX.Element | null {
  const show = usePrefsStore((state) => state.prefs.general.showUsageInfo);
  const navigation = usePrefsStore((state) => state.prefs.view3d.navigation);
  if (!show) {
    return null;
  }
  return (
    <ul aria-label={strings.usageInfo.label} className="usage-info">
      {usageLines(mode, navigation, moveKey).map((line) => (
        <li className="usage-info__item" key={line}>
          {line}
        </li>
      ))}
    </ul>
  );
}
