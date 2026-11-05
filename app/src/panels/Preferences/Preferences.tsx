import {
  useEffect,
  useId,
  useRef,
  useState,
  type ChangeEvent,
  type FormEvent,
  type JSX,
  type KeyboardEvent,
} from 'react';

import type {
  Background3d,
  GridStyle,
  IntersectionCheck,
  Material3d,
  Navigation3d,
  Preferences as Prefs,
  RenderMode,
  Shading,
  SnapDefaults,
} from '../../bridge/prefs';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { prefsStrings } from '../../lib/strings/prefs';
import { usePrefsStore } from '../../state/prefsStore';
import { MATERIALS } from '../../views/View3D/appearance';
import {
  draftFrom,
  draftPatch,
  draftProblems,
  editDraft,
  NUMBER_RULES,
  PREFS_SECTIONS,
  type NumberField,
  type PrefsDraft,
  type PrefsSection,
} from './model';

import './Preferences.scss';
import { dialogKeyDown } from '../../lib/dialogKeys';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

const text = prefsStrings.preferences;

/** The Preferences dialog, modal over the window. */
export function Preferences(): JSX.Element {
  const open = usePrefsStore((state) => state.dialogOpen);
  const close = usePrefsStore((state) => state.closeDialog);
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (element === null) {
      return;
    }
    if (open && !element.open) {
      element.showModal();
    } else if (!open && element.open) {
      element.close();
    }
  }, [open]);

  return (
    <dialog
      aria-labelledby="preferences-title"
      className="preferences"
      onClose={close}
      onKeyDown={dialogKeyDown(close)}
      ref={dialog}
    >
      {open ? <PreferencesBody onClose={close} /> : null}
    </dialog>
  );
}

function PreferencesBody({ onClose }: { onClose: () => void }): JSX.Element {
  const prefs = usePrefsStore((state) => state.prefs);
  const error = usePrefsStore((state) => state.error);
  const path = usePrefsStore((state) => state.path);
  const setAside = usePrefsStore((state) => state.setAside);
  const update = usePrefsStore((state) => state.update);
  const reset = usePrefsStore((state) => state.reset);
  const [draft, setDraft] = useState<PrefsDraft>(() => draftFrom(prefs));
  const [section, setSection] = useState<PrefsSection>('general');
  const tabs = useRef<Partial<Record<PrefsSection, HTMLButtonElement | null>>>({});
  const problems = draftProblems(draft);
  const valid = Object.keys(problems).length === 0;
  const okWhy = valid ? undefined : strings.why.fixValues;

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const patch = draftPatch(draft);
    if (patch !== null && (await update(patch))) {
      onClose();
    }
  };

  const onReset = async () => {
    if (await reset()) {
      setDraft(draftFrom(usePrefsStore.getState().prefs));
    }
  };

  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    const index = PREFS_SECTIONS.indexOf(section);
    const count = PREFS_SECTIONS.length;
    const next =
      event.key === 'ArrowDown' || event.key === 'ArrowRight'
        ? PREFS_SECTIONS[(index + 1) % count]
        : event.key === 'ArrowUp' || event.key === 'ArrowLeft'
          ? PREFS_SECTIONS[(index + count - 1) % count]
          : event.key === 'Home'
            ? PREFS_SECTIONS[0]
            : event.key === 'End'
              ? PREFS_SECTIONS[count - 1]
              : undefined;
    if (next !== undefined) {
      event.preventDefault();
      setSection(next);
      tabs.current[next]?.focus();
    }
  };

  const fields: FieldProps = { draft, setDraft, problems };

  return (
    <form className="preferences__body" onSubmit={(event) => void onSubmit(event)}>
      <header className="preferences__header">
        <h2 className="preferences__title" id="preferences-title">
          {text.title}
        </h2>
      </header>
      <div className="preferences__panes">
        <div
          aria-label={text.sections}
          aria-orientation="vertical"
          className="preferences__tabs"
          role="tablist"
        >
          {PREFS_SECTIONS.map((id) => (
            <button
              aria-controls={`preferences-panel-${id}`}
              aria-selected={section === id}
              className="preferences__tab"
              id={`preferences-tab-${id}`}
              key={id}
              onClick={() => setSection(id)}
              onKeyDown={onTabKey}
              ref={(element) => {
                tabs.current[id] = element;
              }}
              role="tab"
              tabIndex={section === id ? 0 : -1}
              type="button"
            >
              {text.sectionNames[id]}
            </button>
          ))}
        </div>
        <div
          aria-labelledby={`preferences-tab-${section}`}
          className="preferences__panel"
          id={`preferences-panel-${section}`}
          role="tabpanel"
        >
          {section === 'general' ? <GeneralSection {...fields} /> : null}
          {section === 'view2d' ? <View2dSection {...fields} /> : null}
          {section === 'view3d' ? <View3dSection {...fields} /> : null}
          {section === 'toolpaths' ? <ToolpathsSection {...fields} /> : null}
          {section === 'confirmations' ? <ConfirmationsSection {...fields} /> : null}
        </div>
      </div>
      {setAside !== null ? <p className="preferences__note">{text.setAside(setAside)}</p> : null}
      {error !== null ? (
        <p className="preferences__error" role="alert">
          {error}
        </p>
      ) : null}
      <footer className="preferences__footer">
        <p className="preferences__path">{path === null ? text.notSaved : text.savedIn(path)}</p>
        <button
          className="preferences__button"
          onClick={() => void onReset()}
          type="button"
          data-tip={text.reset}
        >
          <ButtonFace icon="formReset" label={text.reset} />
        </button>
        <button
          className="preferences__button"
          onClick={onClose}
          type="button"
          data-tip={text.cancel}
        >
          <ButtonFace icon="dismiss" label={text.cancel} />
        </button>
        <button
          className="preferences__button preferences__button--primary"
          disabled={okWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.ok, okWhy)}
        >
          <ButtonFace icon="formApply" label={text.ok} />
        </button>
      </footer>
    </form>
  );
}

interface FieldProps {
  draft: PrefsDraft;
  setDraft: (update: (draft: PrefsDraft) => PrefsDraft) => void;
  problems: Partial<Record<NumberField, string>>;
}

function NumberInput({
  field,
  label,
  unit,
  draft,
  setDraft,
  problems,
}: FieldProps & { field: NumberField; label: string; unit?: string }): JSX.Element {
  const id = useId();
  const problem = problems[field];
  const rule = NUMBER_RULES[field];
  const hint = `${id}-hint`;
  return (
    <div className="preferences__row">
      <label className="preferences__label" htmlFor={id}>
        {label}
      </label>
      <span className="preferences__control">
        <input
          aria-describedby={problem === undefined ? undefined : hint}
          aria-invalid={problem !== undefined}
          className="preferences__number"
          id={id}
          inputMode="decimal"
          onChange={(event: ChangeEvent<HTMLInputElement>) => {
            const value = event.target.value;
            setDraft((d) => ({ ...d, numbers: { ...d.numbers, [field]: value } }));
          }}
          type="text"
          value={draft.numbers[field]}
        />
        {unit === undefined ? null : <span className="preferences__unit">{unit}</span>}
      </span>
      {problem === undefined ? null : (
        <span className="preferences__problem" id={hint}>
          {text.range(rule.min, rule.max)}
        </span>
      )}
    </div>
  );
}

function Check({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}): JSX.Element {
  const id = useId();
  return (
    <div className="preferences__row preferences__row--check">
      <input
        checked={checked}
        className="preferences__check"
        id={id}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
      <label className="preferences__check-label" htmlFor={id}>
        {label}
      </label>
    </div>
  );
}

function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
}): JSX.Element {
  const id = useId();
  return (
    <div className="preferences__row">
      <label className="preferences__label" htmlFor={id}>
        {label}
      </label>
      <select
        className="preferences__select"
        id={id}
        onChange={(event) => {
          const picked = options.find((o) => o.value === event.target.value);
          if (picked !== undefined) {
            onChange(picked.value);
          }
        }}
        value={value}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}

function Colour({
  label,
  value,
  onChange,
  disabled = false,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}): JSX.Element {
  const id = useId();
  return (
    <div className="preferences__row">
      <label className="preferences__label" htmlFor={id}>
        {label}
      </label>
      <input
        className="preferences__colour"
        disabled={disabled}
        id={id}
        onChange={(event) => onChange(event.target.value)}
        type="color"
        value={value}
      />
    </div>
  );
}

function useSection<S extends Exclude<keyof Prefs, 'version'>>(
  props: FieldProps,
  section: S,
): [Prefs[S], (patch: Partial<Prefs[S]>) => void] {
  return [
    props.draft.prefs[section],
    (patch) => props.setDraft((d) => editDraft(d, section, patch)),
  ];
}

function GeneralSection(props: FieldProps): JSX.Element {
  const [general, setGeneral] = useSection(props, 'general');
  const [layout, setLayout] = useSection(props, 'window');
  const units: readonly { value: Units; label: string }[] = [
    { value: 'mm', label: strings.units.mmLong },
    { value: 'inch', label: strings.units.inchLong },
  ];
  const recentCount = props.draft.clearRecent ? 0 : general.recentFiles.length;
  const clearWhy = recentCount === 0 ? strings.why.nothingRecent : undefined;
  return (
    <>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.newJobs}</legend>
        <Choice
          label={text.defaultUnits}
          onChange={(defaultUnits) => setGeneral({ defaultUnits })}
          options={units}
          value={general.defaultUnits}
        />
        <Check
          checked={layout.jobSetupAtStart}
          label={text.jobSetupAtStart}
          onChange={(jobSetupAtStart) => setLayout({ jobSetupAtStart })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.editing}</legend>
        <NumberInput {...props} field="undoLimit" label={text.undoLimit} />
        <NumberInput {...props} field="autosaveMinutes" label={text.autosave} unit={text.minutes} />
        <Check
          checked={general.showToolTips}
          label={text.showToolTips}
          onChange={(showToolTips) => setGeneral({ showToolTips })}
        />
        <Check
          checked={general.showUsageInfo}
          label={text.showUsageInfo}
          onChange={(showUsageInfo) => setGeneral({ showUsageInfo })}
        />
        <Check
          checked={general.graphicsTablet}
          label={text.graphicsTablet}
          onChange={(graphicsTablet) => setGeneral({ graphicsTablet })}
        />
        <Check
          checked={general.showFileOperations}
          label={text.showFileOperations}
          onChange={(showFileOperations) => setGeneral({ showFileOperations })}
        />
        <Check
          checked={general.smoothJoin}
          label={text.smoothJoin}
          onChange={(smoothJoin) => setGeneral({ smoothJoin })}
        />
        <Choice
          label={text.smoothJoinMethod}
          onChange={(smoothJoinMethod) => setGeneral({ smoothJoinMethod })}
          options={[
            { value: 'smoother', label: text.smoothJoinSmoother },
            { value: 'shallow', label: text.smoothJoinShallow },
          ]}
          value={general.smoothJoinMethod}
        />
        <Check
          checked={general.lastTextAnchor}
          label={text.lastTextAnchor}
          onChange={(lastTextAnchor) => setGeneral({ lastTextAnchor })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.recentFiles}</legend>
        <NumberInput {...props} field="recentFilesLength" label={text.recentFilesLength} />
        <div className="preferences__row">
          <span className="preferences__label">{text.recentCount(recentCount)}</span>
          <button
            className="preferences__button"
            disabled={clearWhy !== undefined}
            onClick={() => props.setDraft((d) => ({ ...d, clearRecent: true }))}
            type="button"
            data-tip={whyTip(text.clearRecent, clearWhy)}
          >
            <ButtonFace icon="clearRecent" label={text.clearRecent} />
          </button>
        </div>
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.windowLayout}</legend>
        <Check
          checked={layout.remember}
          label={text.rememberLayout}
          onChange={(remember) => setLayout({ remember })}
        />
        <Colour
          label={text.topRulerColour}
          onChange={(topRulerColour) => setLayout({ topRulerColour })}
          value={layout.topRulerColour}
        />
        <Colour
          label={text.bottomRulerColour}
          onChange={(bottomRulerColour) => setLayout({ bottomRulerColour })}
          value={layout.bottomRulerColour}
        />
      </fieldset>
    </>
  );
}

const SNAP_KEYS: readonly (keyof SnapDefaults)[] = [
  'grid',
  'guides',
  'nodes',
  'midpoints',
  'centres',
  'intersections',
  'jobPoints',
  'smart',
];

const GRID_STYLES: readonly { value: GridStyle; label: string }[] = [
  { value: 'points', label: text.gridPoints },
  { value: 'lines', label: text.gridLines },
];

function View2dSection(props: FieldProps): JSX.Element {
  const [view2d, setView2d] = useSection(props, 'view2d');
  return (
    <>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.display}</legend>
        <Check
          checked={view2d.showGrid}
          label={text.showGrid}
          onChange={(showGrid) => setView2d({ showGrid })}
        />
        <NumberInput {...props} field="gridSpacing" label={text.gridSpacing} unit={text.mm} />
        <Choice
          label={text.gridStyle}
          onChange={(gridStyle) => setView2d({ gridStyle })}
          options={GRID_STYLES}
          value={view2d.gridStyle}
        />
        <Check
          checked={view2d.showRulers}
          label={text.showRulers}
          onChange={(showRulers) => setView2d({ showRulers })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.snapDefaults}</legend>
        {SNAP_KEYS.map((key) => (
          <Check
            checked={view2d.snap[key]}
            key={key}
            label={text.snapNames[key]}
            onChange={(on) => setView2d({ snap: { ...view2d.snap, [key]: on } })}
          />
        ))}
        <NumberInput {...props} field="snapRadiusPx" label={text.snapRadius} unit={text.pixels} />
      </fieldset>
    </>
  );
}

function View3dSection(props: FieldProps): JSX.Element {
  const [view3d, setView3d] = useSection(props, 'view3d');
  const backgrounds: readonly { value: Background3d; label: string }[] = [
    { value: 'theme', label: text.backgroundTheme },
    { value: 'gradient', label: text.backgroundGradient },
    { value: 'solid', label: text.backgroundSolid },
  ];
  const shadings: readonly { value: Shading; label: string }[] = [
    { value: 'smooth', label: text.shadingSmooth },
    { value: 'flat', label: text.shadingFlat },
  ];
  const renderModes: readonly { value: RenderMode; label: string }[] = [
    { value: 'shaded', label: text.renderShaded },
    { value: 'wireframe', label: text.renderWireframe },
    { value: 'shadedWire', label: text.renderShadedWire },
  ];
  const materials: readonly { value: Material3d; label: string }[] = MATERIALS.map((value) => ({
    value,
    label: text.materials[value],
  }));
  const schemes: readonly { value: Navigation3d; label: string }[] = [
    { value: 'modern', label: text.navigationModern },
    { value: 'legacy', label: text.navigationLegacy },
  ];
  return (
    <>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.background}</legend>
        <Choice
          label={text.backgroundStyle}
          onChange={(background) => setView3d({ background })}
          options={backgrounds}
          value={view3d.background}
        />
        <Colour
          disabled={view3d.background === 'theme'}
          label={view3d.background === 'solid' ? text.backgroundColour : text.topColour}
          onChange={(topColour) => setView3d({ topColour })}
          value={view3d.topColour}
        />
        <Colour
          disabled={view3d.background !== 'gradient'}
          label={text.bottomColour}
          onChange={(bottomColour) => setView3d({ bottomColour })}
          value={view3d.bottomColour}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.rendering}</legend>
        <Choice
          label={text.shading}
          onChange={(shading) => setView3d({ shading })}
          options={shadings}
          value={view3d.shading}
        />
        <Choice
          label={text.renderMode}
          onChange={(renderMode) => setView3d({ renderMode })}
          options={renderModes}
          value={view3d.renderMode}
        />
        <Choice
          label={text.material}
          onChange={(material) => setView3d({ material })}
          options={materials}
          value={view3d.material}
        />
        <Colour
          disabled={view3d.material !== 'solid'}
          label={text.solidColour}
          onChange={(solidColour) => setView3d({ solidColour })}
          value={view3d.solidColour}
        />
        <Check
          checked={view3d.animateCamera}
          label={text.animateCamera}
          onChange={(animateCamera) => setView3d({ animateCamera })}
        />
        <Check
          checked={view3d.showCube}
          label={text.showCube}
          onChange={(showCube) => setView3d({ showCube })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.navigation}</legend>
        <Choice
          label={text.navigationScheme}
          onChange={(navigation) => setView3d({ navigation })}
          options={schemes}
          value={view3d.navigation}
        />
      </fieldset>
    </>
  );
}

const FILTERS: readonly { value: 'activeSheet' | 'allSheets'; label: string }[] = [
  { value: 'activeSheet', label: text.filterActiveSheet },
  { value: 'allSheets', label: text.filterAllSheets },
];

function ToolpathsSection(props: FieldProps): JSX.Element {
  const [toolpaths, setToolpaths] = useSection(props, 'toolpaths');
  return (
    <>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.calculation}</legend>
        <NumberInput {...props} field="tolerance" label={text.tolerance} unit={text.mm} />
        <NumberInput {...props} field="tolerance3d" label={text.tolerance3d} unit={text.mm} />
        <NumberInput
          {...props}
          field="vgrooveTolerance"
          label={text.vgrooveTolerance}
          unit={text.mm}
        />
        <Check
          checked={toolpaths.dropTool}
          label={text.dropTool}
          onChange={(dropTool) => setToolpaths({ dropTool })}
        />
        <NumberInput
          {...props}
          field="geometryFixSeconds"
          label={text.geometryFixSeconds}
          unit={text.seconds}
        />
        <Check
          checked={toolpaths.newRaster}
          label={text.newRaster}
          onChange={(newRaster) => setToolpaths({ newRaster })}
        />
        <Check
          checked={toolpaths.checkAllDepths}
          label={text.checkAllDepths}
          onChange={(checkAllDepths) => setToolpaths({ checkAllDepths })}
        />
        <Check
          checked={toolpaths.askBeforeRecalculating}
          label={text.askBeforeRecalculating}
          onChange={(askBeforeRecalculating) => setToolpaths({ askBeforeRecalculating })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.toolpathList}</legend>
        <Choice
          label={text.defaultFilter}
          onChange={(defaultFilter) => setToolpaths({ defaultFilter })}
          options={FILTERS}
          value={toolpaths.defaultFilter}
        />
        <Check
          checked={toolpaths.selectSheetOnEdit}
          label={text.selectSheetOnEdit}
          onChange={(selectSheetOnEdit) => setToolpaths({ selectSheetOnEdit })}
        />
        <Check
          checked={toolpaths.appendDuplicated}
          label={text.appendDuplicated}
          onChange={(appendDuplicated) => setToolpaths({ appendDuplicated })}
        />
        <Check
          checked={toolpaths.appendToSelectedGroup}
          label={text.appendToSelectedGroup}
          onChange={(appendToSelectedGroup) => setToolpaths({ appendToSelectedGroup })}
        />
        <Check
          checked={toolpaths.showOperationsWithPreview}
          label={text.showOperationsWithPreview}
          onChange={(showOperationsWithPreview) => setToolpaths({ showOperationsWithPreview })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.previewColours}</legend>
        <Colour
          label={text.cutColour}
          onChange={(cutColour) => setToolpaths({ cutColour })}
          value={toolpaths.cutColour}
        />
        <Colour
          label={text.rapidColour}
          onChange={(rapidColour) => setToolpaths({ rapidColour })}
          value={toolpaths.rapidColour}
        />
        <Colour
          label={text.solidPreviewColour}
          onChange={(solidPreviewColour) => setToolpaths({ solidPreviewColour })}
          value={toolpaths.solidPreviewColour}
        />
      </fieldset>
    </>
  );
}

const CHECKS: readonly { value: IntersectionCheck; label: string }[] = [
  { value: 'ask', label: text.intersectionAsk },
  { value: 'validator', label: text.intersectionValidator },
  { value: 'skip', label: text.intersectionSkip },
];

const SHARP_CHECKS: readonly { value: IntersectionCheck; label: string }[] = [
  ...CHECKS,
  { value: 'proceed', label: text.intersectionProceed },
];

function ConfirmationsSection(props: FieldProps): JSX.Element {
  const [confirmations, setConfirmations] = useSection(props, 'confirmations');
  return (
    <>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.askBefore}</legend>
        <Check
          checked={confirmations.deleteLayer}
          label={text.confirmDeleteLayer}
          onChange={(deleteLayer) => setConfirmations({ deleteLayer })}
        />
        <Check
          checked={confirmations.deleteSheet}
          label={text.confirmDeleteSheet}
          onChange={(deleteSheet) => setConfirmations({ deleteSheet })}
        />
        <Check
          checked={confirmations.deleteTool}
          label={text.confirmDeleteTool}
          onChange={(deleteTool) => setConfirmations({ deleteTool })}
        />
        <Check
          checked={confirmations.embossWarning}
          label={text.confirmEmboss}
          onChange={(embossWarning) => setConfirmations({ embossWarning })}
        />
      </fieldset>
      <fieldset className="preferences__group">
        <legend className="preferences__legend">{text.intersections}</legend>
        <Choice
          label={text.toolpathIntersections}
          onChange={(toolpathIntersections) => setConfirmations({ toolpathIntersections })}
          options={CHECKS}
          value={confirmations.toolpathIntersections}
        />
        <Choice
          label={text.radiusedOffsetIntersections}
          onChange={(radiusedOffsetIntersections) =>
            setConfirmations({ radiusedOffsetIntersections })
          }
          options={CHECKS}
          value={confirmations.radiusedOffsetIntersections}
        />
        <Choice
          label={text.sharpOffsetIntersections}
          onChange={(sharpOffsetIntersections) => setConfirmations({ sharpOffsetIntersections })}
          options={SHARP_CHECKS}
          value={confirmations.sharpOffsetIntersections}
        />
      </fieldset>
    </>
  );
}
