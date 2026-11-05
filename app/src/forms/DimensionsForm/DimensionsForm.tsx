import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type JSX,
} from 'react';

import { api } from '../../bridge/formApi';
import type { DimensionStyleSettings, EntityJson, Path, Point } from '../../bridge/document';
import { nearestArc } from '../../lib/spans';
import { SPELL_CHECKED } from '../../lib/spelling';
import { strings } from '../../lib/formStrings';
import { lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { unitSuffix } from '../../lib/units';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SelectField } from '../SelectField/SelectField';
import {
  clearPlaced,
  convertDimensionUnits,
  dragDimensionText,
  initialDimensionState,
  isComplete,
  onDimensionText,
  PAIRS,
  pickCircle,
  placePoint,
  placingPreview,
  pointCount,
  pointLabels,
  specFromPoints,
  specFromState,
  stateFromDimension,
  textAtOf,
  typedPoints,
  validateDimension,
  type DimensionField,
  type DimensionKind,
  customTextFor,
  editedTextAt,
  type DimensionFormState,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DimensionsForm.scss';

const text = strings.createVectors.dimensions;

/** The dimension types, each with its shell/ToolIcons icon. */
const KINDS: ReadonlyArray<{ value: DimensionKind; label: string; icon: string }> = [
  { value: 'linear', label: text.linear, icon: 'dimLinear' },
  { value: 'vertical', label: text.vertical, icon: 'dimVertical' },
  { value: 'horizontal', label: text.horizontal, icon: 'dimensions' },
  { value: 'angular', label: text.angular, icon: 'dimAngular' },
  { value: 'arc', label: text.arc, icon: 'dimArc' },
];

type SettingKey = 'textHeight' | 'precision' | 'anglePrecision' | 'offset';

/** The number settings: lengths in the job unit, or whole numbers of places. */
const SETTING_FIELDS: ReadonlyArray<{
  key: SettingKey;
  length: boolean;
  valid: (value: number) => boolean;
}> = [
  { key: 'textHeight', length: true, valid: (v) => v > 0 },
  { key: 'precision', length: false, valid: (v) => Number.isInteger(v) && v >= 0 && v <= 6 },
  { key: 'anglePrecision', length: false, valid: (v) => Number.isInteger(v) && v >= 0 && v <= 6 },
  { key: 'offset', length: true, valid: (v) => v >= 0 },
];

/** One Decimal Places box: the angle's places for an Angle dimension, the length's otherwise. */
function shownSettings(kind: string): typeof SETTING_FIELDS {
  const hidden = kind === 'angular' ? 'precision' : 'anglePrecision';
  return SETTING_FIELDS.filter((field) => field.key !== hidden);
}

/** A dimension whose text is being dragged, and where it was grabbed. */
interface TextDrag {
  entity: Extract<EntityJson, { kind: 'dimension' }>;
  grab: Point;
  moved: boolean;
}

/** The text box of a dragged dimension, moved with the pointer, as a preview. */
function dragPreview(drag: TextDrag, pointer: Point): Path[] {
  const box = drag.entity.textBox;
  const dx = pointer.x - drag.grab.x;
  const dy = pointer.y - drag.grab.y;
  const guide: Path = {
    subpaths: [{ start: drag.grab, closed: false, segments: [{ kind: 'line', to: pointer }] }],
  };
  if (box === undefined) {
    return [guide];
  }
  const [x0, y0, x1, y1] = [box[0] + dx, box[1] + dy, box[2] + dx, box[3] + dy];
  const outline: Path = {
    subpaths: [
      {
        start: { x: x0, y: y0 },
        closed: true,
        segments: [
          { kind: 'line', to: { x: x1, y: y0 } },
          { kind: 'line', to: { x: x1, y: y1 } },
          { kind: 'line', to: { x: x0, y: y1 } },
        ],
      },
    ],
  };
  return [guide, outline];
}

export function DimensionsForm(): JSX.Element {
  // Menu, form and panel icons load after start up; redraw once they are in.
  useExtraIcons();
  const createDimension = useDocumentStore((store) => store.createDimension);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialDimensionState,
    convertDimensionUnits,
  );
  const [cursor, setCursor] = useState<Point | null>(null);
  const [showErrors, setShowErrors] = useState(false);
  const customId = useId();
  const layerId = useId();
  const [layerDraft, setLayerDraft] = useState<string | null>(null);
  const [families, setFamilies] = useState<string[]>([]);
  const [settings, setSettings] = useState<DimensionStyleSettings | null>(null);
  const [editing, setEditing] = useState<{
    id: number;
    loaded: DimensionFormState;
    textAt: Point | undefined;
  } | null>(null);
  const [drafts, setDrafts] = useState<Partial<Record<SettingKey, string>>>({});
  const textDrag = useRef<TextDrag | null>(null);
  const [dragPaths, setDragPaths] = useState<Path[] | null>(null);

  useEffect(() => {
    let live = true;
    void api.text
      .fonts()
      .then((reply) => {
        if (live) {
          setFamilies(reply.families);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    void api.dims
      .style()
      .then((reply) => {
        if (live) {
          setSettings(reply);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  const changeSettings = (changes: Partial<DimensionStyleSettings>) => {
    void api.dims
      .style(changes)
      .then(setSettings)
      .catch(() => undefined);
  };

  const errors = useMemo(() => validateDimension(state, units), [state, units]);
  const placing = useMemo(() => placingPreview(state, cursor), [state, cursor]);
  usePreview(dragPaths ?? placing);

  const create = useCallback(async () => {
    setShowErrors(true);
    const spec = specFromState(stateRef.current, unitsRef.current);
    const points = typedPoints(stateRef.current, unitsRef.current) ?? [];
    if (spec !== null) {
      await createDimension(
        spec,
        customTextFor(stateRef.current),
        textAtOf(stateRef.current.kind, points),
      );
    }
  }, [createDimension, stateRef, unitsRef]);

  const controller = useMemo<DrawController>(
    () => ({
      pointerDown: (p) => {
        textDrag.current = null;
        if (stateRef.current.placed.length > 0) {
          return;
        }
        // Pressing on the text of an existing dimension starts dragging it.
        const grab = { x: p.rawX, y: p.rawY };
        const tolerance = 3 / useViewStore.getState().camera.pxPerMm;
        void api.entities.hitTest({ ...grab, tolerance }).then(async ({ ids }) => {
          for (const id of ids) {
            const entity = await api.entities.get(id);
            if (entity.kind === 'dimension' && onDimensionText(entity.textBox, grab, tolerance)) {
              textDrag.current = { entity, grab, moved: false };
              return;
            }
          }
        });
      },
      pointerMove: (p) => {
        setCursor({ x: p.x, y: p.y });
        const drag = textDrag.current;
        if (drag !== null) {
          const pointer = { x: p.rawX, y: p.rawY };
          const px = useViewStore.getState().camera.pxPerMm;
          if (Math.hypot(pointer.x - drag.grab.x, pointer.y - drag.grab.y) * px > 3) {
            drag.moved = true;
          }
          if (drag.moved) {
            setDragPaths(dragPreview(drag, pointer));
          }
        }
      },
      pointerUp: (p) => {
        const at = { x: p.x, y: p.y };
        const tolerance = 6 / useViewStore.getState().camera.pxPerMm;
        const drag = textDrag.current;
        textDrag.current = null;
        if (drag !== null && drag.moved) {
          setDragPaths(null);
          const moved = dragDimensionText(drag.entity.spec, drag.grab, at, p.shift);
          void api.dims
            .move(drag.entity.id, moved.spec, moved.textAt)
            .then(() => useDocumentStore.getState().refresh());
          return;
        }
        if (p.shift) {
          // Shift click picks an existing dimension to edit it.
          void api.entities.hitTest({ x: p.rawX, y: p.rawY, tolerance }).then(async ({ ids }) => {
            for (const id of ids) {
              const entity = await api.entities.get(id);
              if (entity.kind !== 'dimension') {
                continue;
              }
              const loaded = stateFromDimension(
                stateRef.current,
                unitsRef.current,
                entity.spec,
                entity.customText ?? '',
                entity.textAt,
              );
              if (loaded !== null) {
                setEditing({ id, loaded, textAt: entity.textAt });
                setState(loaded);
                await useDocumentStore.getState().select([id], 'set');
                return;
              }
            }
          });
          return;
        }
        const current = stateRef.current;
        if (current.kind === 'arc' && current.placed.length === 0) {
          // The first click picks the arc or circle to measure.
          void api.entities.hitTest({ x: p.rawX, y: p.rawY, tolerance }).then(async ({ ids }) => {
            for (const id of ids) {
              const entity = await api.entities.get(id);
              const circle = entity.kind === 'path' ? nearestArc(entity.path, at) : null;
              if (circle !== null) {
                setState((s) => pickCircle(s, unitsRef.current, circle.centre, circle.radius));
                return;
              }
            }
          });
          return;
        }
        const placed = placePoint(current, unitsRef.current, at);
        if (!isComplete(placed)) {
          setState(placed);
          return;
        }
        setState(clearPlaced(placed));
        const radius = lengthValue(placed.radius, unitsRef.current) ?? 0;
        const spec = specFromPoints(placed.kind, placed.placed, radius, placed.showDiameter);
        if (spec === null) {
          setShowErrors(true);
          return;
        }
        void createDimension(spec, customTextFor(placed), textAtOf(placed.kind, placed.placed));
      },
      keyDown: (event) => {
        if (event.key === 'Enter') {
          void create();
          return true;
        }
        return false;
      },
      cancel: () => {
        textDrag.current = null;
        setDragPaths(null);
        setState((s) => clearPlaced(s));
      },
    }),
    [create, createDimension, setState, stateRef, unitsRef],
  );
  useDrawController(controller);

  const setField = (field: DimensionField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: DimensionField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);
  const labels = pointLabels(state.kind);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  return (
    <form className="dimensions" onSubmit={onSubmit}>
      <p className="dimensions__hint">{text.hint(labels.join(', '))}</p>
      <fieldset className="dimensions__group">
        <legend className="dimensions__legend">{text.kind}</legend>
        <div className="dimensions__kinds">
          {KINDS.map((kind) => (
            <label className="dimensions__kind" key={kind.value}>
              <input
                checked={state.kind === kind.value}
                className="dimensions__kind-radio"
                name="dimensionKind"
                onChange={() => setState((s) => clearPlaced({ ...s, kind: kind.value }))}
                type="radio"
                value={kind.value}
              />
              <span aria-hidden="true" className="dimensions__kind-icon">
                {toolIcon(kind.icon)}
              </span>
              <span className="dimensions__kind-label">{kind.label}</span>
            </label>
          ))}
        </div>
      </fieldset>
      {PAIRS.slice(0, pointCount(state.kind)).map(([xField, yField], index) => (
        <fieldset className="dimensions__group" key={xField}>
          <legend className="dimensions__legend">{labels[index]}</legend>
          <NumberField
            error={errorFor(xField)}
            label="X"
            onChange={(v) => setField(xField, v)}
            unit={unit}
            value={state[xField]}
          />
          <NumberField
            error={errorFor(yField)}
            label="Y"
            onChange={(v) => setField(yField, v)}
            unit={unit}
            value={state[yField]}
          />
        </fieldset>
      ))}
      {state.kind === 'arc' ? (
        <>
          <NumberField
            error={showErrors ? errors.radius : undefined}
            label={text.radiusValue}
            onChange={(v) => setState((s) => ({ ...s, radius: v }))}
            unit={unit}
            value={state.radius}
          />
          <CheckField
            checked={state.showDiameter}
            label={text.showDiameter}
            onChange={(showDiameter) => setState((s) => ({ ...s, showDiameter }))}
          />
        </>
      ) : null}
      <div className="dimensions__note">
        <CheckField
          checked={state.useCustom}
          label={text.useCustom}
          onChange={(useCustom) => setState((s) => ({ ...s, useCustom }))}
        />
        <label className="dimensions__label" htmlFor={customId}>
          {text.customText}
        </label>
        <input
          className="dimensions__input"
          disabled={!state.useCustom}
          id={customId}
          onChange={(event) => setState((s) => ({ ...s, customText: event.target.value }))}
          type="text"
          value={state.customText}
          {...SPELL_CHECKED}
        />
        <p className="dimensions__hint">{editing === null ? text.customHint : text.editing}</p>
        {editing !== null ? (
          <button
            className="dimensions__button"
            onClick={() => {
              const spec = specFromState(state, units);
              if (spec === null) {
                setShowErrors(true);
                return;
              }
              const textAt = editedTextAt(state, units, editing.loaded, editing.textAt);
              void api.dims
                .edit(editing.id, spec, textAt, customTextFor(state) ?? '')
                .then(() => useDocumentStore.getState().refresh());
            }}
            type="button"
            data-tip={strings.form.apply}
          >
            <ButtonFace icon="formApply" label={strings.form.apply} />
          </button>
        ) : (
          <p className="dimensions__hint">{text.editHint}</p>
        )}
      </div>
      {settings !== null ? (
        <fieldset className="dimensions__group">
          <legend className="dimensions__legend">{text.settings}</legend>
          <SelectField
            label={text.font}
            onChange={(fontFamily) => changeSettings({ fontFamily })}
            options={(families.includes(settings.fontFamily)
              ? families
              : [settings.fontFamily, ...families]
            ).map((family) => ({ value: family, label: family }))}
            value={settings.fontFamily}
          />
          {shownSettings(state.kind).map((field) => (
            <NumberField
              key={field.key}
              label={field.key === 'anglePrecision' ? text.precision : text[field.key]}
              onChange={(v) => {
                setDrafts((current) => ({ ...current, [field.key]: v }));
                const value = field.length ? lengthValue(v, units) : numberValue(v);
                if (value !== null && field.valid(value)) {
                  changeSettings({ [field.key]: value });
                }
              }}
              step={field.length ? undefined : 1}
              unit={field.length ? unit : undefined}
              value={
                drafts[field.key] ??
                (field.length
                  ? lengthText(settings[field.key], units)
                  : String(settings[field.key]))
              }
            />
          ))}
        </fieldset>
      ) : null}
      {settings !== null ? (
        <fieldset className="dimensions__group">
          <legend className="dimensions__legend">{text.useLayer}</legend>
          <CheckField
            checked={settings.useLayer}
            label={text.useLayer}
            onChange={(useLayer) => changeSettings({ useLayer })}
          />
          <div className="dimensions__note">
            <label className="dimensions__label" htmlFor={layerId}>
              {text.layerName}
            </label>
            <input
              className="dimensions__input"
              disabled={!settings.useLayer}
              id={layerId}
              onChange={(event) => setLayerDraft(event.target.value)}
              onBlur={() => {
                if (layerDraft !== null && layerDraft.trim() !== '') {
                  changeSettings({ layerName: layerDraft.trim() });
                }
                setLayerDraft(null);
              }}
              type="text"
              value={layerDraft ?? settings.layerName}
            />
          </div>
        </fieldset>
      ) : null}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void create()}
        primaryLabel={strings.form.create}
        submit
      />
    </form>
  );
}
