import type { JSX } from 'react';

import type { SnapOptions } from '../../bridge/document';
import { strings } from '../../lib/strings';
import { useViewStore } from '../../state/viewStore';
import { SMART_SNAP_ICON, SNAP_GRID_ICON, SNAP_OBJECTS_ICON } from '../SnapBar/icons';
import { OBJECT_SNAPS, objectSnapToggle } from '../SnapBar/model';

// The toggle icons are drawn with the 2D view toolbar's icon classes.
import '../SnapBar/SnapBar.scss';
import './SnapToggles.scss';

const text = strings.snapBar;

const TOGGLES: ReadonlyArray<{ key: keyof SnapOptions; label: string; icon: JSX.Element }> = [
  { key: 'smart', label: text.smart, icon: SMART_SNAP_ICON },
  { key: 'grid', label: text.grid, icon: SNAP_GRID_ICON },
];

/** The three snapping toggles at the right of the document bar: geometry, smart, grid. */
export function SnapToggles(): JSX.Element {
  const snap = useViewStore((state) => state.snap);
  const setSnap = useViewStore((state) => state.setSnap);
  const objectsOn = OBJECT_SNAPS.some((key) => snap[key]);
  return (
    <div aria-label={text.snapGroup} className="snap-toggles" role="group">
      <button
        aria-label={text.objects}
        aria-pressed={objectsOn}
        className="snap-toggles__button"
        onClick={() => setSnap(objectSnapToggle(snap))}
        data-tip={text.objects}
        type="button"
      >
        {SNAP_OBJECTS_ICON}
      </button>
      {TOGGLES.map((toggle) => (
        <button
          aria-label={toggle.label}
          aria-pressed={snap[toggle.key]}
          className="snap-toggles__button"
          key={toggle.key}
          onClick={() => setSnap({ [toggle.key]: !snap[toggle.key] })}
          data-tip={toggle.label}
          type="button"
        >
          {toggle.icon}
        </button>
      ))}
    </div>
  );
}
