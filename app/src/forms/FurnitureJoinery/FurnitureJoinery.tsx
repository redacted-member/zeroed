import { useCallback, useEffect, useId, useState, type JSX } from 'react';

import { toolDiameter, type Tool } from '../../bridge/cam';
import { api } from '../../bridge/formApi';
import {
  JOINERY_STYLES,
  type ConnectionRow,
  type FurnitureJoint,
  type FurnitureJoints,
  type JoineryStyle,
  type JointClass,
} from '../../bridge/furniture';
import type { Fit, JointParams, JointSettings, Relief } from '../../bridge/joints';
import type { Units } from '../../bridge/types';
import { lengthText } from '../../lib/formFields';
import { furnitureStrings } from '../../lib/strings/furniture';
import { jointsStrings } from '../../lib/strings/joints';
import { unitSuffix } from '../../lib/units';
import { drawIcon } from '../../shell/ToolIcons/ToolIcons';
import { IconButton } from '../IconButton/IconButton';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { jointFit } from '../HelperPicture/pictures/furniture';
import { HelperRow } from '../HelperRow/HelperRow';
import { fitLabel } from '../JointCreatorForm/model';
import { NumberField } from '../NumberField/NumberField';
import { SelectField } from '../SelectField/SelectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import {
  classRows,
  jointSizeFields,
  sizesOf,
  sizeTexts,
  type ClassRow,
} from '../FurnitureDesigner/model';
import { jointIcon, jointText } from '../FurnitureDesigner/jointItems';

import './FurnitureJoinery.scss';

const text = furnitureStrings.furniture;
const jt = jointsStrings.joints;

/** The joint each style shows as its picture. */
const STYLE_ICON: Readonly<Record<JoineryStyle, FurnitureJoint>> = {
  template: 'dado',
  traditional: 'dovetail',
  flatPack: 'camLock',
  quickBuild: 'dowel',
  laser: 'finger',
  glueScrews: 'none',
};

const FITS: readonly Fit[] = ['slip', 'press', 'glue', 'knockDown'];
const RELIEFS: ReadonlyArray<{ value: Relief; icon: string }> = [
  { value: 'none', icon: 'reliefNone' },
  { value: 'dogBone', icon: 'reliefDogBone' },
  { value: 'tBone', icon: 'reliefTBone' },
];

interface FurnitureJoineryProps {
  classes: Readonly<Record<string, JointClass>> | undefined;
  connections: readonly ConnectionRow[];
  joints: FurnitureJoints;
  units: Units;
  disabled: boolean;
  onStyle: (style: JoineryStyle) => void;
  onClassJoint: (cls: string, joint: FurnitureJoint | null, params?: JointParams) => void;
  /** The shared fit, relief or tool changed: the design is made again. */
  onSettings: () => void;
}

/**
 * Joinery: a style for the whole design, a row per class with the joint's
 * picture and the joints valid for it (sizes under a disclosure), and the
 * fit, corner relief and tool every joint shares with the Joint Creator.
 */
export function FurnitureJoinery(props: FurnitureJoineryProps): JSX.Element {
  const { classes, connections, joints, units, disabled, onStyle, onClassJoint, onSettings } =
    props;
  const rows = classRows(classes, connections, joints, (cls) =>
    cls === 'other' ? text.designer.otherClass : cls,
  );
  return (
    <fieldset className="furniture-joinery" disabled={disabled}>
      <legend className="furniture-joinery__legend">{text.designer.joinery}</legend>
      <div className="furniture-joinery__style">
        <span aria-hidden="true" className="furniture-joinery__picture">
          {drawIcon(jointIcon(STYLE_ICON[joints.style]))}
        </span>
        <SelectField
          label={text.designer.style}
          onChange={onStyle}
          options={JOINERY_STYLES.map((style) => ({
            value: style,
            label: text.styles[style].label,
          }))}
          value={joints.style}
        />
      </div>
      <p className="furniture-joinery__hint">{text.styles[joints.style].tip}</p>
      <p className="furniture-joinery__hint">{text.designer.styleHint}</p>
      {rows.map((row) => (
        // A new joint, new sizes or another unit start the row's boxes again.
        <ClassJointRow
          key={`${row.cls}:${row.joint}:${JSON.stringify(row.params)}:${units}`}
          onClassJoint={onClassJoint}
          row={row}
          units={units}
        />
      ))}
      <SharedSettings onSettings={onSettings} units={units} />
    </fieldset>
  );
}

function ClassJointRow({
  row,
  units,
  onClassJoint,
}: {
  row: ClassRow;
  units: Units;
  onClassJoint: FurnitureJoineryProps['onClassJoint'];
}): JSX.Element {
  const [sizes, setSizes] = useState<Record<string, string>>(() =>
    sizeTexts(row.joint, row.params, units),
  );
  const [bad, setBad] = useState<string[]>([]);
  const paramsKey = JSON.stringify(row.params);

  const commitSizes = () => {
    const parsed = sizesOf(row.joint, sizes, units);
    if (!parsed.ok) {
      setBad(parsed.bad);
      return;
    }
    setBad([]);
    if (JSON.stringify(parsed.params) !== paramsKey) {
      onClassJoint(row.cls, row.joint, parsed.params);
    }
  };
  const fields = jointSizeFields(row.joint);
  const unit = unitSuffix(units);

  return (
    <div className="furniture-joinery__row">
      <span aria-hidden="true" className="furniture-joinery__picture">
        {drawIcon(jointIcon(row.joint))}
      </span>
      <div className="furniture-joinery__pick">
        <SelectField
          label={row.label}
          onChange={(joint) => onClassJoint(row.cls, joint)}
          options={row.choices.map((joint) => ({ value: joint, label: jointText(joint).name }))}
          value={row.joint}
        />
        <p className="furniture-joinery__source">
          {row.source === 'class' ? text.designer.fromClass : text.designer.fromStyle}
          {row.overridden.length > 0 ? `, ${text.designer.mixed(row.overridden.length)}` : ''}
        </p>
        {fields.length === 0 ? null : (
          <details className="furniture-joinery__sizes">
            <summary className="furniture-joinery__summary">{text.designer.editSizes}</summary>
            <p className="furniture-joinery__hint">{text.designer.sizesHint}</p>
            <div className="furniture-joinery__size-fields" onBlur={commitSizes}>
              {fields.map((spec) => (
                <NumberField
                  error={bad.includes(spec.key) ? text.designer.badValue : undefined}
                  key={spec.key}
                  label={jt.labels[spec.label ?? spec.key]}
                  onChange={(typed) => setSizes((current) => ({ ...current, [spec.key]: typed }))}
                  onEnter={commitSizes}
                  placeholder={jt.auto}
                  unit={
                    spec.unit === 'length' ? unit : spec.unit === 'angle' ? jt.degrees : undefined
                  }
                  value={sizes[spec.key] ?? ''}
                />
              ))}
            </div>
          </details>
        )}
      </div>
    </div>
  );
}

/** Fit, corner relief and tool: the Joint Creator's settings, shared by every joint. */
function SharedSettings({
  units,
  onSettings,
}: {
  units: Units;
  onSettings: () => void;
}): JSX.Element {
  const [settings, setSettings] = useState<JointSettings | null>(null);
  const [tool, setTool] = useState<Tool | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const headingId = useId();

  useEffect(() => {
    let live = true;
    void api.joints
      .list()
      .then((list) => {
        if (live) {
          setSettings(list.settings);
        }
      })
      .catch((error: unknown) =>
        setProblem(error instanceof Error ? error.message : String(error)),
      );
    return () => {
      live = false;
    };
  }, []);

  const save = useCallback(
    async (next: JointSettings) => {
      setSettings(next);
      try {
        await api.joints.setSettings(next);
        setProblem(null);
        onSettings();
      } catch (error) {
        setProblem(error instanceof Error ? error.message : String(error));
      }
    },
    [onSettings],
  );

  if (settings === null) {
    return <p className="furniture-joinery__hint">{problem ?? ''}</p>;
  }
  return (
    <section aria-labelledby={headingId} className="furniture-joinery__settings">
      <h3 className="furniture-joinery__heading" id={headingId}>
        {text.designer.fitAndTool}
      </h3>
      <p className="furniture-joinery__hint">{text.designer.fitHint}</p>
      <HelperRow picture={<HelperPicture picture={jointFit} />}>
        <SelectField
          label={jt.fit}
          onChange={(fit) => void save({ ...settings, fit })}
          options={[...FITS, ...(settings.fit === 'custom' ? (['custom'] as const) : [])].map(
            (fit) => ({
              value: fit,
              label: fitLabel(fit, units),
            }),
          )}
          value={settings.fit}
        />
      </HelperRow>
      <div className="furniture-joinery__relief">
        <span className="furniture-joinery__relief-label">{jt.relief}</span>
        <div className="furniture-joinery__relief-buttons">
          {RELIEFS.map((relief) => (
            <IconButton
              icon={relief.icon}
              key={relief.value}
              label={jt.reliefs[relief.value]}
              onClick={() => void save({ ...settings, relief: relief.value })}
              pressed={settings.relief === relief.value}
              title={jt.reliefTitles[relief.value]}
            />
          ))}
        </div>
      </div>
      <ToolPicker
        kinds={['endMill']}
        onChange={(picked) => {
          setTool(picked);
          const diameter = toolDiameter(picked);
          if (diameter > 0 && Math.abs(diameter - settings.toolDiameter) > 1e-9) {
            void save({ ...settings, toolDiameter: diameter });
          }
        }}
        prefer="endMill"
        preferDiameter={settings.toolDiameter}
        tool={tool}
        units={units}
      />
      <p className="furniture-joinery__hint">
        {jt.toolDiameter(lengthText(settings.toolDiameter, units) + unitSuffix(units))}
      </p>
      {problem === null ? null : (
        <p className="furniture-joinery__error" role="alert">
          {problem}
        </p>
      )}
    </section>
  );
}
