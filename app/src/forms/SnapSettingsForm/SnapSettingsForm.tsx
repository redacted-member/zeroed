import { useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  snapSettingsFromView,
  snapSettingsValues,
  toggleSnap,
  validateSnapSettings,
  type SnapSettingsField,
  type SnapToggle,
} from './model';

import './SnapSettingsForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { snapPicture } from '../HelperPicture/pictures/drawing';

const GEOMETRY_TOGGLES: readonly SnapToggle[] = [
  { key: 'nodes', label: strings.snapBar.nodes },
  { key: 'midpoints', label: strings.snapBar.midpoints },
  { key: 'centres', label: strings.snapBar.centres },
  { key: 'intersections', label: strings.snapBar.intersections },
  { key: 'jobPoints', label: strings.snapBar.jobPoints },
];

const GUIDE_TOGGLES: readonly SnapToggle[] = [
  { key: 'guides', label: strings.snapBar.guides },
  { key: 'grid', label: strings.snapBar.grid },
  { key: 'smart', label: strings.snapBar.smart },
];

export function SnapSettingsForm(): JSX.Element {
  const units = useDocumentStore((store) => store.job.units);
  const closeForm = useUiStore((store) => store.closeForm);
  const view = useViewStore();
  const text = strings.snapSettings;
  const [state, setState] = useState(() =>
    snapSettingsFromView(
      view.snap,
      view.gridSpacing,
      view.snapRadiusPx,
      units,
      view.snapDistances,
      view.nudgeDistance,
    ),
  );
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateSnapSettings(state, units), [state, units]);

  const apply = () => {
    setShowErrors(true);
    const values = snapSettingsValues(state, units);
    if (values === null) {
      return;
    }
    view.setSnap(values.snap);
    view.setGridSpacing(values.gridSpacingMm);
    view.setSnapRadiusPx(values.snapRadiusPx);
    view.setSnapDistances(values.snapDistances);
    view.setNudgeDistance(values.nudgeDistanceMm);
    // Snap settings are kept for the next session.
    void usePrefsStore.getState().update({
      view2d: {
        snap: values.snap,
        gridSpacing: values.gridSpacingMm,
        snapRadiusPx: values.snapRadiusPx,
        snapDistances: values.snapDistances,
        nudgeDistance: values.nudgeDistanceMm,
      },
    });
    closeForm();
  };

  const errorFor = (field: SnapSettingsField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    apply();
  };

  const toggles = (list: readonly SnapToggle[]) =>
    list.map((toggle) => (
      <label className="snap-settings__choice" key={toggle.key}>
        <input
          checked={state.snap[toggle.key]}
          onChange={() => setState((current) => toggleSnap(current, toggle.key))}
          type="checkbox"
        />
        <HelperPicture picture={snapPicture(toggle.key)} size="icon" />
        {toggle.label}
      </label>
    ));

  return (
    <form className="snap-settings" onSubmit={onSubmit}>
      <fieldset className="snap-settings__group">
        <legend className="snap-settings__legend">{text.geometry}</legend>
        {toggles(GEOMETRY_TOGGLES)}
      </fieldset>
      <fieldset className="snap-settings__group">
        <legend className="snap-settings__legend">{text.guidesAndGrid}</legend>
        {toggles(GUIDE_TOGGLES)}
        <NumberField
          error={errorFor('gridSpacing')}
          label={strings.snapBar.gridSpacing}
          onChange={(gridSpacing) => setState((current) => ({ ...current, gridSpacing }))}
          unit={unitSuffix(units)}
          value={state.gridSpacing}
        />
      </fieldset>
      <fieldset className="snap-settings__group">
        <legend className="snap-settings__legend">{text.radius}</legend>
        <NumberField
          error={errorFor('snapRadiusPx')}
          label={strings.snapBar.snapRadius}
          onChange={(snapRadiusPx) => setState((current) => ({ ...current, snapRadiusPx }))}
          step={1}
          unit="px"
          value={state.snapRadiusPx}
        />
      </fieldset>
      <fieldset className="snap-settings__group">
        <legend className="snap-settings__legend">{text.distances}</legend>
        <label className="snap-settings__choice">
          <input
            checked={state.snapDistances}
            onChange={() =>
              setState((current) => ({ ...current, snapDistances: !current.snapDistances }))
            }
            type="checkbox"
          />
          {text.snapDistances}
        </label>
        <NumberField
          error={errorFor('nudgeDistance')}
          label={text.nudgeDistance}
          onChange={(nudgeDistance) => setState((current) => ({ ...current, nudgeDistance }))}
          unit={unitSuffix(units)}
          value={state.nudgeDistance}
        />
      </fieldset>
      <FormButtons onClose={closeForm} onPrimary={apply} primaryLabel={strings.form.ok} submit />
    </form>
  );
}
