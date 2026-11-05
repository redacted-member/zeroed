import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Reflect } from '../../bridge/model';
import { strings } from '../../lib/formStrings';
import { numberValue } from '../../lib/formFields';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController, PreviewOverlay } from '../../state/drawStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { ModelingActions } from '../ModelingActions/ModelingActions';
import { useModelingApply } from '../ModelingActions/useModelingApply';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  boundaryFromSelection,
  dragTile,
  initialTextureState,
  nextReflect,
  seedOf,
  stateFromTexture,
  textureParams,
  tileGripAt,
  tileOutline,
  validateTexture,
  type TextureField,
  type TextureFormState,
  type TextureMode,
  type TileGrip,
} from './model';

import './TextureAreaForm.scss';

const text = strings.model.textureArea;

const MODES: ReadonlyArray<Choice<TextureMode>> = [
  { value: 'transform', label: text.transformObject },
  { value: 'editTile', label: text.editTile },
];

/** A little F drawn as the reflection shows it. */
function ReflectGlyph({ reflect }: { reflect: Reflect }): JSX.Element {
  const flipX = reflect === 'horizontal' || reflect === 'both' ? -1 : 1;
  const flipY = reflect === 'vertical' || reflect === 'both' ? -1 : 1;
  return (
    <svg aria-hidden="true" className="texture-area__glyph" viewBox="-12 -12 24 24">
      <g transform={`scale(${flipX} ${-flipY})`}>
        <path d="M-5 -8V8H6M-5 0H3" />
      </g>
    </svg>
  );
}

const QUARTERS: ReadonlyArray<0 | 1 | 2 | 3> = [2, 3, 0, 1];

/** Pick distance round the tile's corners, screen pixels. */
const GRIP_PX = 8;

/** Tile box the Edit Textured Area Component mode shows, as an overlay. */
function tileOverlays(state: TextureFormState): PreviewOverlay[] {
  const tile = state.tile;
  if (state.mode !== 'editTile' || tile === null) {
    return [];
  }
  return [{ path: tileOutline(tile, state.angle), tone: 'accent' }];
}

export function TextureAreaForm(): JSX.Element {
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const closeForm = useUiStore((store) => store.closeForm);
  const picked = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const [opened] = useState(() => {
    const model = useModelStore.getState();
    const component = model.components.find((c) => c.id === model.selectedId) ?? null;
    return component === null || (component.texture ?? null) === null ? null : component;
  });
  const [state, setState] = useState<TextureFormState>(() =>
    opened === null ? initialTextureState() : stateFromTexture(opened),
  );
  const { track, apply, reset, startNew, retarget } = useModelingApply(opened?.id ?? null);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateTexture(state), [state]);
  const boundary = boundaryFromSelection(selectedIds, entities);

  // Picking a texture area while the form is open edits it.
  const target = track.target;
  useEffect(() => {
    if (picked !== null && (picked.texture ?? null) !== null && picked.id !== target) {
      void retarget(picked.id).then(() => setState(stateFromTexture(picked)));
    }
  }, [picked, target, retarget]);

  const components = useModelStore((store) => store.components);
  const editing = target === null ? null : (components.find((c) => c.id === target) ?? null);
  const seed = editing?.texture?.seed ?? seedOf(picked);
  const overlays = useMemo(() => tileOverlays(state), [state]);

  const run = (next: TextureFormState) => {
    setShowErrors(true);
    const params = textureParams(next);
    if (params === null || seed === null) {
      return;
    }
    const name = next.name.trim();
    void apply(async (applyTarget) => {
      const reply = await api.model.textureArea({
        component: seed,
        ids: boundary,
        params,
        combine: next.combine,
        ...(name === '' ? {} : { name }),
        ...applyTarget,
      });
      setState((current) => ({ ...current, tile: reply.tile }));
      return reply;
    }).then(() => {
      if (useUiStore.getState().viewMode === '2d') {
        useUiStore.getState().setViewMode('split');
      }
    });
  };
  const runRef = useRef(run);
  useEffect(() => {
    runRef.current = run;
  });

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    run(state);
  };

  // Edit Textured Area Component: drag the tile's corners or middle.
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);
  const grip = useRef<{ grip: TileGrip; start: TextureFormState['tile'] } | null>(null);
  const controller = useMemo((): DrawController => {
    const tolerance = () => GRIP_PX / Math.max(useViewStore.getState().camera.pxPerMm, 1e-9);
    return {
      cursor: 'default',
      pointerDown: (p) => {
        const tile = stateRef.current.tile;
        const at = { x: p.rawX, y: p.rawY };
        const found =
          tile === null ? null : tileGripAt(tile, at, tolerance(), stateRef.current.angle);
        grip.current = found === null ? null : { grip: found, start: tile };
      },
      pointerMove: (p) => {
        const held = grip.current;
        if (held === null || held.start === null) {
          return;
        }
        const tile = dragTile(
          held.start,
          held.grip,
          { x: p.rawX, y: p.rawY },
          stateRef.current.angle,
        );
        setState((current) => ({ ...current, tile }));
      },
      pointerUp: () => {
        if (grip.current !== null) {
          grip.current = null;
          runRef.current(stateRef.current);
        }
      },
      cancel: () => {
        grip.current = null;
      },
    };
  }, []);

  return (
    <form className="texture-area" onSubmit={onSubmit}>
      <ChoiceField
        label={text.mode}
        onChange={(mode) => setState((current) => ({ ...current, mode }))}
        options={MODES}
        value={state.mode}
      />
      {state.mode === 'editTile' ? (
        <TileEditing controller={controller} overlays={overlays} />
      ) : null}
      <p className="texture-area__hint">
        {(editing?.texture ?? null) === null ? text.hint : text.editing}
      </p>
      <p className={seed === null ? 'texture-area__notice' : 'texture-area__hint'}>
        {seed === null ? text.needsComponent : text.area(boundary.length)}
      </p>
      <fieldset className="texture-area__group">
        <legend className="texture-area__legend">{text.spacing}</legend>
        <Percent
          error={showErrors ? errors.spacingX : undefined}
          field="spacingX"
          label={text.x}
          max={100}
          min={-50}
          setState={setState}
          value={state.spacingX}
        />
        <Percent
          error={showErrors ? errors.spacingY : undefined}
          field="spacingY"
          label={text.y}
          max={100}
          min={-50}
          setState={setState}
          value={state.spacingY}
        />
      </fieldset>
      <fieldset className="texture-area__group">
        <legend className="texture-area__legend">{text.shift}</legend>
        <Percent
          error={showErrors ? errors.shiftX : undefined}
          field="shiftX"
          label={text.xShift}
          max={100}
          min={0}
          setState={setState}
          value={state.shiftX}
        />
        <Percent
          error={showErrors ? errors.shiftY : undefined}
          field="shiftY"
          label={text.yShift}
          max={100}
          min={0}
          setState={setState}
          value={state.shiftY}
        />
      </fieldset>
      <fieldset className="texture-area__group">
        <legend className="texture-area__legend">{text.reflection}</legend>
        <div className="texture-area__reflect">
          {QUARTERS.map((quarter) => (
            <button
              aria-label={`${text.quarters[quarter]}: ${text.reflects[state.reflect[quarter]]}`}
              className="texture-area__reflect-button"
              key={quarter}
              onClick={() => setState((current) => nextReflect(current, quarter))}
              data-tip={text.reflects[state.reflect[quarter]]}
              type="button"
            >
              <ReflectGlyph reflect={state.reflect[quarter]} />
            </button>
          ))}
        </div>
      </fieldset>
      <ModelingActions
        applyWhy={seed === null ? text.needsComponent : undefined}
        combine={state.combine}
        name={state.name}
        onClose={closeForm}
        onCombine={(combine) => setState((current) => ({ ...current, combine }))}
        onName={(name) => setState((current) => ({ ...current, name }))}
        onReset={() => void reset()}
        onStartNew={() => void startNew()}
      />
    </form>
  );
}

/** Registers the tile editing controller and overlay while shown. */
function TileEditing({
  controller,
  overlays,
}: {
  controller: DrawController;
  overlays: PreviewOverlay[];
}): null {
  useDrawController(controller);
  useOverlays(overlays);
  return null;
}

function Percent({
  label,
  field,
  value,
  min,
  max,
  error,
  setState,
}: {
  label: string;
  field: TextureField;
  value: string;
  min: number;
  max: number;
  error: string | undefined;
  setState: (update: (current: TextureFormState) => TextureFormState) => void;
}): JSX.Element {
  const set = (v: string) => setState((current) => ({ ...current, [field]: v }));
  const n = numberValue(value) ?? 0;
  return (
    <>
      <NumberField error={error} label={label} onChange={set} unit="%" value={value} />
      <SliderField
        label={label}
        max={max}
        min={min}
        onChange={(v) => set(String(v))}
        unit="%"
        value={Math.min(max, Math.max(min, n))}
      />
    </>
  );
}
