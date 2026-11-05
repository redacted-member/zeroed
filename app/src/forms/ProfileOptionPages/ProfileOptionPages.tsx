import type { JSX } from 'react';

import type { LeadKind, RampKind, StartAt } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { AnchorPicker } from '../AnchorPicker/AnchorPicker';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import {
  cornersPicture,
  leadPicture,
  rampPicture,
  tabsPicture,
} from '../HelperPicture/pictures/toolpaths';
import { HelperRow } from '../HelperRow/HelperRow';
import { NumberField } from '../NumberField/NumberField';
import { OptionTabs, type OptionTab } from '../OptionTabs/OptionTabs';
import {
  leadArcLength,
  ORDER_OPTIONS,
  rampFields,
  spiralAllowed,
  toggleOrder,
  type OrderOption,
  type ProfileErrors,
  type ProfileField,
  type ProfileFormState,
  type RampBy,
} from '../ProfileToolpathForm/model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ProfileOptionPages.scss';

const text = strings.toolpaths;

const LEADS: ReadonlyArray<Choice<LeadKind>> = [
  { value: 'none', label: text.profile.leadNone },
  { value: 'straight', label: text.profile.leadStraight },
  { value: 'arc', label: text.profile.leadArc },
];

const RAMP_BY: ReadonlyArray<Choice<RampBy>> = [
  { value: 'distance', label: text.rampByDistance },
  { value: 'angle', label: text.rampByAngle },
];

const STARTS: ReadonlyArray<Choice<StartAt>> = [
  { value: 'vectorStart', label: text.profile.startVector },
  { value: 'optimized', label: text.profile.startOptimized },
  { value: 'boundingBox', label: text.profile.startBox },
];

const ORDER_LABELS: Record<OrderOption, string> = {
  selectionOrder: text.profile.orderSelection,
  leftToRight: text.profile.orderLeftToRight,
  bottomToTop: text.profile.orderBottomToTop,
  grid: text.profile.orderGrid,
};

function rampKinds(spiral: boolean): ReadonlyArray<Choice<RampKind>> {
  return [
    { value: 'smooth', label: text.rampSmooth },
    { value: 'zigZag', label: text.rampZigZag },
    { value: 'spiral', label: text.rampSpiral, disabled: !spiral },
  ];
}

interface ProfileOptionPagesProps {
  state: ProfileFormState;
  onChange: (update: (current: ProfileFormState) => ProfileFormState) => void;
  errors: ProfileErrors;
  units: Units;
  /** Opens the Edit Tabs panel. */
  onEditTabs: () => void;
  /** The Corners page, on the Profile form only: whether the tool is a V-bit. */
  corners?: { vBit: boolean };
}

/**
 * The Profile form's tabs section and its Ramp, Leads, Order, Start At and
 * Corners pages, shared by the Profile form and the forms that cut a
 * profile of their own (the inlay insert and hole).
 */
export function ProfileOptionPages({
  state,
  onChange,
  errors,
  units,
  onEditTabs,
  corners,
}: ProfileOptionPagesProps): JSX.Element {
  const unit = unitSuffix(units);
  const patch = (change: Partial<ProfileFormState>) =>
    onChange((current) => ({ ...current, ...change }));
  const set = (field: ProfileField, value: string) => patch({ [field]: value });
  const ramp = rampFields(state);
  const arcLength = leadArcLength(state, units);

  const rampsPage = (
    <>
      <CheckField
        checked={state.ramp}
        label={text.addRamps}
        onChange={(on) => patch({ ramp: on })}
      />
      <HelperRow picture={<HelperPicture picture={rampPicture(state.rampKind)} />}>
        <ChoiceField
          disabled={!state.ramp}
          label={text.rampKind}
          onChange={(rampKind) => patch({ rampKind })}
          options={rampKinds(spiralAllowed(state))}
          value={state.rampKind}
        />
      </HelperRow>
      <CheckField
        checked={state.rampOnLeadIn}
        disabled={!state.ramp || state.leadKind === 'none'}
        label={text.rampOnLeadIn}
        onChange={(rampOnLeadIn) => patch({ rampOnLeadIn })}
      />
      <ChoiceField
        disabled={!ramp.by}
        label={text.rampBy}
        onChange={(rampBy) => patch({ rampBy })}
        options={RAMP_BY}
        value={state.rampBy}
      />
      <NumberField
        disabled={!ramp.distance}
        error={errors.rampDistance}
        label={text.rampDistance}
        onChange={(v) => set('rampDistance', v)}
        unit={unit}
        value={state.rampDistance}
      />
      <NumberField
        disabled={!ramp.angle}
        error={errors.rampAngle}
        label={text.rampAngle}
        onChange={(v) => set('rampAngle', v)}
        unit={strings.units.degrees}
        value={state.rampAngle}
      />
    </>
  );

  const leadsPage = (
    <>
      <HelperRow picture={<HelperPicture picture={leadPicture(state.leadKind)} />}>
        <ChoiceField
          label={text.profile.leadKind}
          onChange={(leadKind) =>
            onChange((current) => ({
              ...current,
              leadKind,
              // A spiral ramp cannot share the loop with a lead.
              rampKind:
                leadKind !== 'none' && current.rampKind === 'spiral' ? 'smooth' : current.rampKind,
            }))
          }
          options={LEADS}
          value={state.leadKind}
        />
      </HelperRow>
      {state.leadKind === 'straight' ? (
        <>
          <NumberField
            error={errors.leadLength}
            label={text.profile.leadLength}
            onChange={(v) => set('leadLength', v)}
            unit={unit}
            value={state.leadLength}
          />
          <NumberField
            error={errors.leadAngle}
            label={text.profile.leadAngle}
            onChange={(v) => set('leadAngle', v)}
            unit={strings.units.degrees}
            value={state.leadAngle}
          />
        </>
      ) : null}
      {state.leadKind === 'arc' ? (
        <>
          <NumberField
            error={errors.leadRadius}
            label={text.profile.leadRadius}
            onChange={(v) => set('leadRadius', v)}
            unit={unit}
            value={state.leadRadius}
          />
          <NumberField
            error={errors.leadArcAngle}
            label={text.profile.leadArcAngle}
            onChange={(v) => set('leadArcAngle', v)}
            unit={strings.units.degrees}
            value={state.leadArcAngle}
          />
          <p className="profile-options__hint">
            {text.profile.leadArcLength(
              arcLength === null ? '-' : `${formatLength(arcLength, units)} ${unit}`,
            )}
          </p>
        </>
      ) : null}
      {state.leadKind === 'none' ? null : (
        <>
          <CheckField
            checked={state.leadOut}
            label={text.profile.leadOut}
            onChange={(leadOut) => patch({ leadOut })}
          />
          <NumberField
            error={errors.leadOvercut}
            label={text.profile.leadOvercut}
            onChange={(v) => set('leadOvercut', v)}
            unit={unit}
            value={state.leadOvercut}
          />
        </>
      )}
      <p className="profile-options__hint">{text.profile.leadHint}</p>
    </>
  );

  const orderPage = (
    <>
      {ORDER_OPTIONS.map((option) => (
        <CheckField
          checked={state.orders.includes(option)}
          key={option}
          label={ORDER_LABELS[option]}
          onChange={(on) =>
            onChange((current) => ({ ...current, orders: toggleOrder(current, option, on) }))
          }
        />
      ))}
      <p className="profile-options__hint">{text.profile.orderHint}</p>
    </>
  );

  const startPage = (
    <>
      <ChoiceField
        label={text.profile.startAt}
        onChange={(startAt) => patch({ startAt })}
        options={STARTS}
        value={state.startAt}
      />
      <AnchorPicker
        disabled={state.startAt !== 'boundingBox'}
        label={text.profile.startBoxPoint}
        onChange={(startAnchor) => patch({ startAnchor })}
        positions="nine"
        value={state.startAnchor}
      />
    </>
  );

  const cornersPage =
    corners === undefined ? null : (
      <HelperRow picture={<HelperPicture picture={cornersPicture(state.sharpCorners)} />}>
        <CheckField
          checked={state.sharpCorners}
          disabled={state.side === 'on'}
          label={text.profile.sharpCorners}
          onChange={(sharpCorners) => patch({ sharpCorners })}
        />
        <CheckField
          checked={state.sharpInternalCorners}
          disabled={state.side === 'on' || !corners.vBit}
          label={text.profile.sharpInternalCorners}
          onChange={(sharpInternalCorners) => patch({ sharpInternalCorners })}
        />
        {corners.vBit ? null : (
          <p className="profile-options__hint">{text.profile.sharpInternalHint}</p>
        )}
      </HelperRow>
    );

  const pages: Array<OptionTab<string>> = [
    { id: 'ramps', label: text.ramps, content: rampsPage },
    { id: 'leads', label: text.profile.lead, content: leadsPage },
    { id: 'order', label: text.profile.order, content: orderPage },
    { id: 'start', label: text.profile.startAt, content: startPage },
  ];
  if (cornersPage !== null) {
    pages.push({ id: 'corners', label: text.profile.corners, content: cornersPage });
  }

  const tabsWhy = state.tabs ? undefined : strings.why.tickFirst(text.profile.addTabs);
  return (
    <>
      <fieldset className="profile-options__group" aria-label={text.profile.tabs}>
        <CheckField
          checked={state.tabs}
          label={text.profile.addTabs}
          onChange={(tabs) => patch({ tabs })}
        />
        <HelperRow picture={<HelperPicture picture={tabsPicture(state.tabTriangular)} />}>
          <NumberField
            disabled={!state.tabs}
            error={errors.tabLength}
            label={text.profile.tabLength}
            onChange={(v) => set('tabLength', v)}
            unit={unit}
            value={state.tabLength}
          />
          <NumberField
            disabled={!state.tabs}
            error={errors.tabThickness}
            label={text.profile.tabThickness}
            onChange={(v) => set('tabThickness', v)}
            unit={unit}
            value={state.tabThickness}
          />
          <CheckField
            checked={state.tabTriangular}
            disabled={!state.tabs}
            label={text.profile.tabs3d}
            onChange={(tabTriangular) => patch({ tabTriangular })}
          />
        </HelperRow>
        <div className="profile-options__row">
          <span className="profile-options__hint">
            {text.profile.tabsPlaced(state.tabPositions?.length ?? null)}
          </span>
          <button
            className="profile-options__button"
            disabled={tabsWhy !== undefined}
            onClick={onEditTabs}
            type="button"
            data-tip={whyTip(text.profile.editTabs, tabsWhy)}
          >
            <ButtonFace icon="itemEdit" label={text.profile.editTabs} />
          </button>
        </div>
      </fieldset>
      <OptionTabs label={text.optionPages} tabs={pages} />
    </>
  );
}
