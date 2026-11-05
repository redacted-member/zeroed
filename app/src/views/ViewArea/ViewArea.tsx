import { Suspense, type JSX } from 'react';

import type { TileDirection } from '../../bridge/prefs';
import { lazyNamed } from '../../lib/lazyNamed';
import { strings } from '../../lib/strings';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { leaveFace, useFaceEditStore } from '../../state/faceEditStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { SideSwitch } from '../SideSwitch/SideSwitch';
import { SnapToggles } from '../SnapToggles/SnapToggles';
import { View2D } from '../View2D/View2D';
import { ViewPickers } from '../ViewPickers/ViewPickers';
import { chooseTab, tabPressed, type ViewTab, ASSEMBLY_FORM } from './viewTabs';

import './ViewArea.scss';

// three.js is large, so the 3D view and its renderer load the first time the
// 3D view is shown.
const View3D = lazyNamed(() => import('../View3D/View3D'), 'View3D');

const TABS: ReadonlyArray<{ mode: ViewTab; label: string; hint: string }> = [
  { mode: '2d', label: strings.views.view2d, hint: strings.views.view2dHint },
  { mode: '3d', label: strings.views.view3d, hint: strings.views.view3dHint },
  { mode: 'assembly', label: strings.views.viewAssembly, hint: strings.views.viewAssemblyHint },
];

const TILES: ReadonlyArray<{ tile: TileDirection; label: string; path: string }> = [
  { tile: 'horizontal', label: strings.views.tileHorizontal, path: 'M3 4h18v16H3zM3 12h18' },
  { tile: 'vertical', label: strings.views.tileVertical, path: 'M3 4h18v16H3zM12 4v16' },
];

/**
 * The document bar (2D View | 3D View, the two tile layouts, the Sheet, Layer
 * and Level drop-downs and the snapping toggles) over the 2D and 3D views,
 * shown alone or tiled.
 */
export function ViewArea(): JSX.Element {
  const viewMode = useUiStore((state) => state.viewMode);
  const setViewMode = useUiStore((state) => state.setViewMode);
  const activeForm = useUiStore((state) => state.activeForm);
  const tile = usePrefsStore((state) => state.prefs.window.tile);
  const updatePrefs = usePrefsStore((state) => state.update);
  const split = viewMode === 'split';
  const stacked = split && tile === 'horizontal';

  const setTile = (next: TileDirection) => {
    if (split && tile === next) {
      setViewMode('2d');
      return;
    }
    setViewMode('split');
    if (tile !== next) {
      void updatePrefs({ window: { tile: next } });
    }
  };

  // The Assembly View tab opens the Assembly View form over the 3D view
  // (the form loads the 3D view itself); 2D View and 3D View close it.
  const chooseView = (tab: ViewTab) => {
    const ui = useUiStore.getState();
    const next = chooseTab(tab, ui.activeForm);
    if (next.form === 'close') {
      ui.closeForm();
    }
    setViewMode(next.viewMode);
    if (next.form === 'open') {
      ui.openForm(ASSEMBLY_FORM);
    }
  };

  const editingFace = useFaceEditStore((state) => state.editing);

  const backToAssembly = () => {
    const ui = useUiStore.getState();
    ui.setViewMode('3d');
    leaveFace(() => ui.openForm(ASSEMBLY_FORM));
  };

  return (
    <section aria-label={strings.views.label} className="view-area">
      <div className="view-area__bar">
        <div aria-label={strings.views.modeLabel} className="view-area__tabs" role="group">
          {TABS.map(({ mode, label, hint }) => (
            <button
              aria-pressed={tabPressed(mode, viewMode, activeForm)}
              className="view-area__tab"
              key={mode}
              onClick={() => chooseView(mode)}
              data-tip={hint}
              data-view={mode}
              type="button"
            >
              {mode === 'assembly' ? (
                <svg
                  aria-hidden="true"
                  className="view-area__tab-icon"
                  focusable="false"
                  viewBox="0 0 16 16"
                >
                  <path d="M2 10.5h8v3H2zM6 3.5h3v7H6zM10 7.5h4v3h-4z" />
                </svg>
              ) : null}
              {label}
            </button>
          ))}
        </div>
        <div aria-label={strings.views.tileLabel} className="view-area__tiles" role="group">
          {TILES.map((option) => (
            <button
              aria-label={option.label}
              aria-pressed={split && tile === option.tile}
              className="view-area__tile"
              key={option.tile}
              onClick={() => setTile(option.tile)}
              data-tip={option.label}
              type="button"
            >
              <svg
                aria-hidden="true"
                className="view-area__tile-icon"
                focusable="false"
                viewBox="0 0 24 24"
              >
                <path d={option.path} />
              </svg>
            </button>
          ))}
        </div>
        <SideSwitch />
        <div className="view-area__pickers">
          <ViewPickers />
        </div>
        <SnapToggles />
      </div>
      {editingFace === null ? null : (
        <div className="view-area__face-edit" role="status">
          <span className="view-area__face-label">
            {strings.views.editingFace(editingFace.label)}
          </span>
          <button
            className="view-area__face-back"
            data-tip={strings.views.backToAssemblyTip}
            onClick={backToAssembly}
            type="button"
          >
            <ButtonFace icon="assemblyView" label={strings.views.backToAssembly} />
          </button>
        </div>
      )}
      <div
        className={`view-area__views view-area__views--${viewMode}${stacked ? ' view-area__views--stacked' : ''}`}
      >
        {viewMode === '3d' ? null : (
          <div className="view-area__pane">
            <View2D />
          </div>
        )}
        {viewMode === '2d' ? null : (
          <div className="view-area__pane">
            <Suspense
              fallback={
                <p className="view-area__loading" role="status">
                  {strings.views.loading3d}
                </p>
              }
            >
              <View3D />
            </Suspense>
          </div>
        )}
      </div>
    </section>
  );
}
