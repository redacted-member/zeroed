import { useId, type JSX, type ReactNode } from 'react';

import type { FurnitureParam, StockThickness } from '../../bridge/furniture';
import type { Units } from '../../bridge/types';
import { furnitureStrings } from '../../lib/strings/furniture';
import { jointsStrings } from '../../lib/strings/joints';
import { unitSuffix } from '../../lib/units';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import type { Picture } from '../HelperPicture/kit';
import { furnitureSize, furnitureThickness } from '../HelperPicture/pictures/furniture';
import { HelperRow } from '../HelperRow/HelperRow';
import { NumberField } from '../NumberField/NumberField';
import {
  paramGroups,
  sliderSpec,
  sliderText,
  sliderValue,
  stockChoices,
} from '../FurnitureDesigner/model';

import './FurnitureParams.scss';

interface FurnitureParamsProps {
  params: readonly FurnitureParam[];
  fields: Readonly<Record<string, string>>;
  errors: Readonly<Record<string, string>>;
  units: Units;
  disabled?: boolean;
  /** Stock thicknesses offered under each thickness parameter. */
  stocks?: readonly StockThickness[];
  /** A field changed; `release` when the change is done (Enter, focus out, slider let go). */
  onChange: (key: string, text: string, release: boolean) => void;
  /** The change in progress is done. */
  onRelease: () => void;
}

/** The helper picture for a group of parameters: the stock beside a thickness, the overall size beside width, depth and height. */
export function groupPicture(params: readonly FurnitureParam[]): Picture | null {
  if (params.some((p) => p.kind === 'thickness')) {
    return furnitureThickness;
  }
  if (params.some((p) => SIZE_KEYS.has(p.key))) {
    return furnitureSize;
  }
  return null;
}

const SIZE_KEYS = new Set(['width', 'depth', 'height', 'length']);

function withPicture(params: readonly FurnitureParam[], fields: ReactNode): ReactNode {
  const picture = groupPicture(params);
  return picture === null ? (
    fields
  ) : (
    <HelperRow picture={<HelperPicture picture={picture} />}>{fields}</HelperRow>
  );
}

/** An option id as a label: "overlay" reads "Overlay". */
function optionLabel(option: string): string {
  return option.length === 0 ? option : option[0]?.toUpperCase() + option.slice(1);
}

/**
 * A template's parameters by group: number boxes that take calculations,
 * a slider under each length and count with bounds, toggles, and choices
 * as segmented buttons.
 */
export function FurnitureParams(props: FurnitureParamsProps): JSX.Element {
  return (
    <>
      {paramGroups(props.params).map((group) => (
        <fieldset className="furniture-params" disabled={props.disabled} key={group.group}>
          {group.group === '' ? null : (
            <legend className="furniture-params__legend">{group.group}</legend>
          )}
          {withPicture(
            group.params,
            group.params.map((param) => <ParamField key={param.key} param={param} {...props} />),
          )}
        </fieldset>
      ))}
    </>
  );
}

function ParamField({
  param,
  fields,
  errors,
  units,
  stocks = [],
  onChange,
  onRelease,
}: FurnitureParamsProps & { param: FurnitureParam }): JSX.Element {
  const id = useId();
  const value = fields[param.key] ?? '';
  if (param.kind === 'toggle') {
    return (
      <label className="furniture-params__toggle">
        <input
          checked={value === '1'}
          onChange={(event) => onChange(param.key, event.target.checked ? '1' : '0', true)}
          type="checkbox"
        />
        {param.label}
      </label>
    );
  }
  if (param.kind === 'choice') {
    return (
      <div className="furniture-params__choice" role="radiogroup" aria-labelledby={`${id}-label`}>
        <span className="furniture-params__label" id={`${id}-label`}>
          {param.label}
        </span>
        <span className="furniture-params__segments">
          {(param.options ?? []).map((option) => (
            <label className="furniture-params__segment" key={option}>
              <input
                checked={value === option}
                className="furniture-params__segment-input"
                name={id}
                onChange={() => onChange(param.key, option, true)}
                type="radio"
                value={option}
              />
              <span className="furniture-params__segment-face">{optionLabel(option)}</span>
            </label>
          ))}
        </span>
      </div>
    );
  }
  const slider = sliderSpec(param, units);
  const choices = stockChoices(param, stocks, units);
  const chosen = choices.find((c) => c.text === value.trim())?.id ?? '';
  const unit =
    param.kind === 'length' || param.kind === 'thickness'
      ? unitSuffix(units)
      : param.kind === 'angle'
        ? jointsStrings.joints.degrees
        : undefined;
  return (
    <div className="furniture-params__number">
      <NumberField
        error={errors[param.key]}
        label={param.label}
        onChange={(typed) => onChange(param.key, typed, false)}
        onEnter={onRelease}
        onLeave={onRelease}
        unit={unit}
        value={value}
      />
      {slider === null ? null : (
        <input
          aria-label={param.label}
          className="furniture-params__slider"
          max={slider.max}
          min={slider.min}
          onChange={(event) =>
            onChange(param.key, sliderText(param, Number(event.target.value), units), false)
          }
          onKeyUp={onRelease}
          onPointerUp={onRelease}
          step={slider.step}
          type="range"
          value={sliderValue(param, value, units)}
        />
      )}
      {choices.length === 0 ? null : (
        <label className="furniture-params__stock" data-tip={furnitureStrings.furniture.stockTip}>
          <span className="furniture-params__label">{furnitureStrings.furniture.stock}</span>
          <select
            className="furniture-params__select"
            onChange={(event) => {
              const pick = choices.find((c) => c.id === event.target.value);
              if (pick !== undefined) {
                onChange(param.key, pick.text, true);
              }
            }}
            value={chosen}
          >
            <option value="">{furnitureStrings.furniture.stockTyped}</option>
            {choices.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </select>
        </label>
      )}
    </div>
  );
}
