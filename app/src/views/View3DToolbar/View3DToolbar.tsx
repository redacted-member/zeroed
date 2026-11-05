import type { JSX } from 'react';

import type { View3dPrefs } from '../../bridge/prefs';
import { strings } from '../../lib/formStrings';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDialogStore } from '../../state/dialogStore';
import { usePrefsStore } from '../../state/prefsStore';
import type { ViewPreset } from '../View3D/camera3d';
import {
  BLOCK_ICON,
  FIT_ICON,
  MODEL_ICON,
  ORTHO_ICON,
  TOOLPATHS_ICON,
  VECTORS_ICON,
  VIEW_ICONS,
} from './icons';

import './View3DToolbar.scss';

const text = strings.view3dBar;

const PRESETS: ReadonlyArray<{ preset: ViewPreset; label: string }> = [
  { preset: 'iso', label: text.iso },
  { preset: 'top', label: text.viewHint(text.top) },
  { preset: 'front', label: text.viewHint(text.front) },
  { preset: 'back', label: text.viewHint(text.back) },
  { preset: 'left', label: text.viewHint(text.left) },
  { preset: 'right', label: text.viewHint(text.right) },
  { preset: 'bottom', label: text.viewHint(text.bottom) },
];

type ShowKey = 'showVectors' | 'showToolpaths' | 'showModel' | 'showBlock';

const SHOW: ReadonlyArray<{ key: ShowKey; label: string; icon: JSX.Element }> = [
  { key: 'showVectors', label: text.showVectors, icon: VECTORS_ICON },
  { key: 'showToolpaths', label: text.showToolpaths, icon: TOOLPATHS_ICON },
  { key: 'showModel', label: text.showModel, icon: MODEL_ICON },
  { key: 'showBlock', label: text.showBlock, icon: BLOCK_ICON },
];

export interface View3DToolbarProps {
  onPreset: (preset: ViewPreset) => void;
  onFit: () => void;
}

/**
 * The icon strip along the top of the 3D view: standard views and zoom to
 * fit, the orthographic toggle, and what the view shows.
 */
export function View3DToolbar({ onPreset, onFit }: View3DToolbarProps): JSX.Element {
  const view3d = usePrefsStore((state) => state.prefs.view3d);
  const update = usePrefsStore((state) => state.update);
  const set = (patch: Partial<View3dPrefs>) => void update({ view3d: patch });
  const ortho = view3d.projection === 'orthographic';
  const assembly = useAssemblyStore((state) => state.active);

  return (
    <div aria-label={text.label} className="view-3d-toolbar" role="toolbar">
      <div aria-label={text.views} className="view-3d-toolbar__group" role="group">
        {PRESETS.map(({ preset, label }) => (
          <button
            aria-label={label}
            className="view-3d-toolbar__button"
            key={preset}
            onClick={() => onPreset(preset)}
            data-tip={label}
            type="button"
          >
            {VIEW_ICONS[preset]}
          </button>
        ))}
        <button
          aria-label={text.fit}
          className="view-3d-toolbar__button"
          onClick={onFit}
          data-tip={text.fit}
          type="button"
        >
          {FIT_ICON}
        </button>
      </div>
      <div className="view-3d-toolbar__group">
        <button
          aria-label={text.orthographic}
          aria-pressed={ortho}
          className="view-3d-toolbar__button"
          onClick={() => set({ projection: ortho ? 'perspective' : 'orthographic' })}
          data-tip={text.orthographic}
          type="button"
        >
          {ORTHO_ICON}
        </button>
      </div>
      <div aria-label={text.show} className="view-3d-toolbar__group" role="group">
        {SHOW.map(({ key, label, icon }) => (
          <button
            aria-label={label}
            aria-pressed={view3d[key]}
            className="view-3d-toolbar__button"
            key={key}
            onClick={() => set({ [key]: !view3d[key] })}
            data-tip={label}
            type="button"
          >
            {icon}
          </button>
        ))}
      </div>
      {assembly ? (
        <div className="view-3d-toolbar__group">
          <button
            className="view-3d-toolbar__text-button"
            onClick={() => useDialogStore.getState().show('furnitureLibrary')}
            data-tip={text.newFurnitureTip}
            type="button"
          >
            <ButtonFace icon="furniture" label={text.newFurniture} />
          </button>
        </div>
      ) : null}
    </div>
  );
}
