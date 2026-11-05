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

import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField } from '../../forms/ChoiceField/ChoiceField';
import { CheckField } from '../../forms/CheckField/CheckField';
import { NumberField } from '../../forms/NumberField/NumberField';
import { DynamicFields } from '../../forms/DynamicFields/DynamicFields';
import type { DynamicState } from '../../forms/DynamicFields/model';
import { treeActions } from '../ComponentsPanel/actions';
import {
  PREVIEW_MATERIALS,
  TREE_COMBINES,
  componentPatch,
  componentState,
  groupPatch,
  groupState,
  levelPatch,
  levelState,
  validateProperties,
  type Appearance,
  type PreviewMaterial,
  type PropertiesState,
  type PropertiesTarget,
} from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ComponentProperties.scss';

const text = strings.model.properties;
const materialNames = text.materials;

/** Wait after the last change before it is applied, ms. */
const SETTLE_MS = 400;

/** The Component Properties form in the Modeling tab: the picked components
 * (it follows the tree's picks while open), a group or a level. Changes
 * apply as they are made, each one undo step; Close keeps them. */
export function ComponentPropertiesForm(): JSX.Element {
  const stored = useModelStore((store) => store.propertiesTarget);
  const selectedId = useModelStore((store) => store.selectedId);
  const picked = useModelStore((store) => store.picked);
  const closeForm = useUiStore((store) => store.closeForm);
  const [generation, setGeneration] = useState(0);
  const pickedIds = pickedComponents({ selectedId, picked });
  const target: PropertiesTarget | null =
    stored?.kind === 'components' && pickedIds.length > 0
      ? { kind: 'components', ids: pickedIds }
      : stored;
  if (target === null) {
    return <p className="component-properties__hint">{text.nothing}</p>;
  }
  return (
    <PropertiesBody
      key={`${JSON.stringify(target)}-${generation}`}
      onBaked={() => setGeneration((g) => g + 1)}
      onClose={closeForm}
      target={target}
    />
  );
}

function PropertiesBody({
  target,
  onClose,
  onBaked,
}: {
  target: PropertiesTarget;
  onClose: () => void;
  /** After Bake: the form starts again from the baked component. */
  onBaked: () => void;
}): JSX.Element {
  const units = useDocumentStore((store) => store.job.units);
  const components = useModelStore((store) => store.components);
  const levels = useModelStore((store) => store.levels);
  const nameId = useId();
  const colourId = useId();
  const materialId = useId();
  const highlight = useModelStore((store) => store.highlight);

  // Turning the highlight off lasts only while the properties are open.
  useEffect(() => () => useModelStore.getState().setHighlight(true), []);

  const chosen = useMemo(
    () => (target.kind === 'components' ? components.filter((c) => target.ids.includes(c.id)) : []),
    [components, target],
  );
  const group =
    target.kind === 'group'
      ? (levels.flatMap((l) => l.groups ?? []).find((g) => g.id === target.id) ?? null)
      : null;
  const level = target.kind === 'level' ? (levels.find((l) => l.id === target.id) ?? null) : null;

  const [initial] = useState<PropertiesState>(() => {
    if (group !== null) {
      return groupState(group, units);
    }
    if (level !== null) {
      return levelState(level, units);
    }
    return componentState(chosen, units);
  });
  const [state, setState] = useState(initial);
  const [failure, setFailure] = useState<string | null>(null);
  const count = chosen.length;
  const errors = validateProperties(state, target.kind, count, units);
  const bakeWhy = Object.keys(errors).length > 0 ? strings.why.fixValues : undefined;
  const unit = unitSuffix(units);
  const set = (patch: Partial<PropertiesState>) =>
    setState((current) => ({ ...current, ...patch }));

  const title =
    target.kind === 'group'
      ? text.groupTitle
      : target.kind === 'level'
        ? text.levelTitle
        : text.title;
  const appearances: ReadonlyArray<{ value: Appearance; label: string }> = [
    target.kind === 'components'
      ? { value: 'parent', label: text.sameAsParent }
      : { value: 'children', label: text.fromChildren },
    { value: 'colour', label: text.colour },
    { value: 'material', label: text.useMaterial },
  ];
  const maxHeight = count === 1 ? (chosen[0]?.maxHeight ?? null) : null;

  const onDynamic = useCallback(
    (dynamic: DynamicState) => setState((current) => ({ ...current, dynamic })),
    [],
  );

  // What the kernel has now; each change applies the difference.
  const committed = useRef(initial);
  const latest = useRef(state);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    latest.current = state;
  }, [state]);

  const apply = async (next: PropertiesState, bake = false): Promise<void> => {
    let ok = true;
    const before = committed.current;
    if (target.kind === 'components') {
      const patch = componentPatch(next, before, count, units);
      if (patch === null) {
        return;
      }
      if (bake) {
        patch.bake = true;
      }
      if (Object.keys(patch).length > 0) {
        ok = await treeActions.setProps(target.ids, patch);
      }
    } else if (target.kind === 'group') {
      const patch = groupPatch(next, before);
      if (patch === null) {
        return;
      }
      if (Object.keys(patch).length > 0) {
        ok = await treeActions.updateGroup(target.id, patch);
      }
    } else {
      const patch = levelPatch(next, before, units);
      if (patch === null) {
        return;
      }
      if (Object.keys(patch).length > 0) {
        await useModelStore.getState().updateLevel(target.id, patch);
        ok = useDocumentStore.getState().lastError === null;
      }
    }
    if (ok) {
      committed.current = next;
      setFailure(null);
      if (bake) {
        onBaked();
      }
    } else {
      setFailure(useDocumentStore.getState().lastError);
    }
  };

  const flush = () => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    void apply(latest.current);
  };

  // Every change applies once the fields settle; Enter, leaving a field
  // or closing applies at once.
  useEffect(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      void apply(latest.current);
    }, SETTLE_MS);
    return () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
      }
    };
    // Only a change of the fields starts the wait.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state]);

  // A change still waiting when the form goes (another pick, Close) applies.
  const applyRef = useRef(apply);
  useEffect(() => {
    applyRef.current = apply;
  });
  useEffect(
    () => () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
        timer.current = null;
        void applyRef.current(latest.current);
      }
    },
    [],
  );

  const close = () => {
    flush();
    onClose();
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    flush();
  };

  return (
    <form className="component-properties__body" onBlur={flush} onSubmit={onSubmit}>
      {target.kind === 'components' ? null : <p className="component-properties__kind">{title}</p>}
      {target.kind === 'components' && count > 1 ? (
        <p className="component-properties__hint">{text.several(count)}</p>
      ) : null}
      {target.kind !== 'components' || count === 1 ? (
        <div className="component-properties__row">
          <label className="component-properties__label" htmlFor={nameId}>
            {text.name}
          </label>
          <input
            aria-describedby={errors.name === undefined ? undefined : `${nameId}-error`}
            aria-invalid={errors.name !== undefined}
            className="component-properties__input"
            id={nameId}
            onChange={(event) => set({ name: event.target.value })}
            type="text"
            value={state.name}
          />
          {errors.name === undefined ? null : (
            <p className="component-properties__error" id={`${nameId}-error`}>
              {errors.name}
            </p>
          )}
        </div>
      ) : null}
      <ChoiceField
        label={text.combine}
        onChange={(combine) => set({ combine })}
        options={TREE_COMBINES}
        value={state.combine}
      />
      {target.kind === 'level' ? (
        <NumberField
          error={errors.baseHeight}
          label={text.baseHeight}
          onChange={(baseHeight) => set({ baseHeight })}
          unit={unit}
          value={state.baseHeight}
        />
      ) : null}
      {target.kind === 'components' ? (
        <>
          <NumberField
            error={errors.shapeHeight}
            label={text.shapeHeight}
            onChange={(shapeHeight) => set({ shapeHeight })}
            step={10}
            unit="%"
            value={state.shapeHeight}
          />
          <NumberField
            error={errors.baseHeight}
            label={text.baseHeight}
            onChange={(baseHeight) => set({ baseHeight })}
            unit={unit}
            value={state.baseHeight}
          />
          {maxHeight === null ? null : (
            <p className="component-properties__hint">
              {text.height(`${formatLength(maxHeight, units)} ${unit}`)}
            </p>
          )}
          <DynamicFields onChange={onDynamic} showErrors state={state.dynamic} />
          <button
            className="component-properties__button"
            disabled={bakeWhy !== undefined}
            onClick={() => void apply(latest.current, true)}
            data-tip={whyTip(text.bakeHint, bakeWhy)}
            type="button"
          >
            <ButtonFace icon="bakeComponents" label={text.bake} />
          </button>
        </>
      ) : null}
      {
        <>
          <ChoiceField
            label={text.appearance}
            onChange={(appearance) => set({ appearance })}
            options={appearances}
            value={state.appearance}
          />
          <div className="component-properties__row">
            <label className="component-properties__label" htmlFor={colourId}>
              {text.pickColour}
            </label>
            <input
              className="component-properties__swatch"
              disabled={state.appearance !== 'colour'}
              id={colourId}
              onChange={(event) => set({ colour: event.target.value })}
              type="color"
              value={state.colour}
            />
          </div>
          <div className="component-properties__row">
            <label className="component-properties__label" htmlFor={materialId}>
              {text.material}
            </label>
            <select
              className="component-properties__input"
              disabled={state.appearance !== 'material'}
              id={materialId}
              onChange={(event) => set({ material: event.target.value as PreviewMaterial })}
              value={state.material}
            >
              {PREVIEW_MATERIALS.map((m) => (
                <option key={m} value={m}>
                  {materialNames[m]}
                </option>
              ))}
            </select>
          </div>
        </>
      }
      {target.kind === 'components' ? (
        <CheckField
          checked={highlight}
          label={text.highlight}
          onChange={(on) => useModelStore.getState().setHighlight(on)}
        />
      ) : null}
      {failure === null ? null : (
        <p className="component-properties__error" role="alert">
          {failure}
        </p>
      )}
      <div className="component-properties__buttons">
        <button
          className="component-properties__button"
          onClick={close}
          type="button"
          data-tip={text.close}
        >
          <ButtonFace icon="dismiss" label={text.close} />
        </button>
      </div>
    </form>
  );
}
