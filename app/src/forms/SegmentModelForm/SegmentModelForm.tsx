import {
  useEffect,
  useState,
  type DragEvent,
  type FormEvent,
  type JSX,
  type MouseEvent,
} from 'react';

import type { SegmentPlan } from '../../bridge/meshImport';
import { numberValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useImportStep } from '../../lib/useImportStep';
import { useDocumentStore } from '../../state/documentStore';
import { useImportStore } from '../../state/importStore';
import { useUiStore } from '../../state/uiStore';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import {
  deleteChildren,
  freeTurn,
  groupOf,
  isCut,
  layoutOf,
  merge,
  planeFieldErrors,
  planeFields,
  planeOf,
  ROOT,
  segmentOfNode,
  separate,
  split,
  togglesFreeRotation,
  treeMenu,
  turnSegment,
  type PlaneFields,
  type TreeAction,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './SegmentModelForm.scss';

const text = strings.model.segmentModel;

type Mode = 'edit' | 'layout';

const MODES: ReadonlyArray<Choice<Mode>> = [
  { value: 'edit', label: text.edit },
  { value: 'layout', label: text.layout },
];

const DRAG_TYPE = 'application/x-carve-segment';

interface Menu {
  id: string;
  x: number;
  y: number;
}

interface FreeFields {
  rotX: string;
  rotY: string;
  rotZ: string;
}

export function SegmentModelForm(): JSX.Element {
  const units = useDocumentStore((store) => store.job.units);
  const openForm = useUiStore((store) => store.openForm);
  const closeForm = useUiStore((store) => store.closeForm);
  const plan = useImportStore((store) => store.plan);
  const view = useImportStore((store) => store.view);
  const info = useImportStore((store) => store.info);
  const busy = useImportStore((store) => store.busy);
  const error = useImportStore((store) => store.error);
  const [typed, setTyped] = useState<PlaneFields | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [free, setFree] = useState(false);
  const [freeFields, setFreeFields] = useState<FreeFields>({ rotX: '0', rotY: '0', rotZ: '0' });
  const [importing, setImporting] = useState(false);
  const actionWhy = firstWhy(
    [importing || busy, strings.why.busy],
    [info === null, strings.why.loading],
  );
  useImportStep('segment');

  const store = useImportStore.getState;
  const mode: Mode = view.layout ? 'layout' : 'edit';
  const selected = view.selected;
  const segment = segmentOfNode(plan, selected);
  const cutWhy = firstWhy(
    [busy, strings.why.busy],
    [isCut(plan, selected), strings.why.segmentCut],
    [groupOf(plan, selected).length > 1, strings.why.segmentMerged],
  );
  const turnWhy = segment === null ? text.selectHint : undefined;

  // The plane starts where the flow put it (level through the middle).
  const fields = typed ?? (view.plane === null ? null : planeFields(view.plane, units));

  // O shows or hides the free rotation fields while laying out.
  useEffect(() => {
    if (mode !== 'layout') {
      return;
    }
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (togglesFreeRotation(event)) {
        setFree((on) => !on);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [mode]);

  // With the free rotation on, the rings in the 3D view turn the chosen
  // segment.
  const ringSegment = mode === 'layout' && free && segment !== null ? segment : null;
  useEffect(() => {
    if (useImportStore.getState().view.ringSegment !== ringSegment) {
      useImportStore.getState().setView({ ringSegment });
      void useImportStore.getState().refresh();
    }
  }, [ringSegment]);
  useEffect(() => () => useImportStore.getState().setView({ ringSegment: null }), []);

  const changePlan = (next: SegmentPlan) => {
    if (next !== store().plan) {
      store().setPlan(next);
      void store().refresh();
    }
  };

  const typePlane = (patch: Partial<PlaneFields>) => {
    if (fields === null) {
      return;
    }
    const next = { ...fields, ...patch };
    setTyped(next);
    const plane = planeOf(next, units);
    if (plane !== null) {
      store().setView({ plane });
    }
  };

  const cut = () => {
    const plane = fields === null ? null : planeOf(fields, units);
    if (plane !== null) {
      changePlan(split(plan, selected, plane));
    }
  };

  const setMode = (next: Mode) => {
    store().setView({ layout: next === 'layout' });
    void store().refresh();
  };

  const select = (id: string) => {
    store().setView({ selected: id });
    const seg = segmentOfNode(plan, id);
    if (seg !== null) {
      const turn = layoutOf(plan, seg);
      setFreeFields({ rotX: String(turn.rotX), rotY: String(turn.rotY), rotZ: String(turn.rotZ) });
    }
  };

  const turn = (axis: 'x' | 'y' | 'z', delta: 1 | -1) => {
    if (segment !== null) {
      changePlan(turnSegment(plan, segment, axis, delta));
    }
  };

  const applyFree = (next: FreeFields) => {
    const [x, y, z] = [numberValue(next.rotX), numberValue(next.rotY), numberValue(next.rotZ)];
    if (segment !== null && x !== null && y !== null && z !== null) {
      changePlan(freeTurn(plan, segment, { rotX: x, rotY: y, rotZ: z }));
    }
  };

  const onMenu = (action: TreeAction) => {
    if (menu === null) {
      return;
    }
    changePlan(
      action.kind === 'deleteChildren' ? deleteChildren(plan, menu.id) : separate(plan, menu.id),
    );
    setMenu(null);
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setImporting(true);
    void store()
      .importSegments()
      .then((ok) => {
        setImporting(false);
        if (ok) {
          closeForm();
        }
      });
  };

  const cancel = () => {
    store().clear();
    closeForm();
  };

  const setOption = (patch: Partial<SegmentPlan>) => {
    store().setPlan({ ...store().plan, ...patch });
  };

  const renderNode = (id: string, depth: number): JSX.Element => {
    const children = isCut(plan, id) ? [`${id}0`, `${id}1`] : [];
    const group = groupOf(plan, id);
    const leaf = children.length === 0;
    const name = id === ROOT ? text.root : text.node(id);
    const onDrop = (event: DragEvent<HTMLButtonElement>) => {
      const from = event.dataTransfer.getData(DRAG_TYPE);
      event.preventDefault();
      if (from !== '' && leaf) {
        changePlan(merge(plan, from, id));
      }
    };
    return (
      <li
        aria-expanded={leaf ? undefined : true}
        aria-level={depth}
        aria-selected={selected === id}
        className="segment-model__node"
        key={id}
        role="treeitem"
      >
        <button
          className={
            selected === id
              ? 'segment-model__node-button segment-model__node-button--selected'
              : 'segment-model__node-button'
          }
          draggable={leaf}
          onClick={() => select(id)}
          onContextMenu={(event: MouseEvent<HTMLButtonElement>) => {
            event.preventDefault();
            select(id);
            setMenu({ id, x: event.clientX, y: event.clientY });
          }}
          onDragOver={(event) => {
            if (leaf) {
              event.preventDefault();
            }
          }}
          onDragStart={(event) => event.dataTransfer.setData(DRAG_TYPE, id)}
          onDrop={onDrop}
          type="button"
        >
          {name}
          {group.length > 1 ? ` ${text.merged(group.length)}` : ''}
        </button>
        {leaf ? null : (
          <ul className="segment-model__children" role="group">
            {children.map((child) => renderNode(child, depth + 1))}
          </ul>
        )}
      </li>
    );
  };

  const errors = fields === null ? {} : planeFieldErrors(fields, units);
  const suffix = unitSuffix(units);
  const layoutTurn = segment === null ? null : layoutOf(plan, segment);
  // Turns made with the rings fill the free rotation fields.
  const turnKey =
    layoutTurn === null ? '' : `${layoutTurn.rotX},${layoutTurn.rotY},${layoutTurn.rotZ}`;
  const [shownTurn, setShownTurn] = useState(turnKey);
  if (turnKey !== shownTurn) {
    setShownTurn(turnKey);
    if (layoutTurn !== null) {
      setFreeFields({
        rotX: String(layoutTurn.rotX),
        rotY: String(layoutTurn.rotY),
        rotZ: String(layoutTurn.rotZ),
      });
    }
  }

  return (
    <form className="segment-model" onSubmit={onSubmit}>
      <p className="segment-model__hint">{text.hint}</p>
      <ChoiceField label={text.mode} onChange={setMode} options={MODES} value={mode} />
      {mode === 'edit' && fields !== null ? (
        <fieldset className="segment-model__group">
          <legend className="segment-model__legend">{text.plane}</legend>
          <NumberField
            error={errors.x}
            label={text.planeX}
            onChange={(x) => typePlane({ x })}
            unit={suffix}
            value={fields.x}
          />
          <NumberField
            error={errors.y}
            label={text.planeY}
            onChange={(y) => typePlane({ y })}
            unit={suffix}
            value={fields.y}
          />
          <NumberField
            error={errors.z}
            label={text.planeZ}
            onChange={(z) => typePlane({ z })}
            unit={suffix}
            value={fields.z}
          />
          <NumberField
            error={errors.angleX}
            label={text.angleX}
            onChange={(angleX) => typePlane({ angleX })}
            step={1}
            unit={text.degrees}
            value={fields.angleX}
          />
          <NumberField
            error={errors.angleY}
            label={text.angleY}
            onChange={(angleY) => typePlane({ angleY })}
            step={1}
            unit={text.degrees}
            value={fields.angleY}
          />
          <p className="segment-model__hint">{text.segmentHint}</p>
          <button
            className="segment-model__button segment-model__button--primary"
            disabled={cutWhy !== undefined}
            onClick={cut}
            type="button"
            data-tip={whyTip(text.segmentAction, cutWhy)}
          >
            <ButtonFace icon="splitComponent" label={text.segmentAction} />
          </button>
        </fieldset>
      ) : null}
      <fieldset className="segment-model__group">
        <legend className="segment-model__legend">{text.tree}</legend>
        <ul aria-label={text.tree} className="segment-model__tree" role="tree">
          {renderNode(ROOT, 1)}
        </ul>
        <p className="segment-model__hint">{text.mergeHint}</p>
      </fieldset>
      {mode === 'layout' ? (
        <fieldset className="segment-model__group">
          <legend className="segment-model__legend">{text.orientation}</legend>
          {segment === null ? <p className="segment-model__hint">{text.selectHint}</p> : null}
          <div className="segment-model__row" role="group" aria-label={text.orientation}>
            <button
              className="segment-model__button"
              disabled={turnWhy !== undefined}
              onClick={() => turn('x', -1)}
              type="button"
              data-tip={whyTip(text.turnUp, turnWhy)}
            >
              <ButtonFace icon="turnUp" label={text.turnUp} />
            </button>
            <button
              className="segment-model__button"
              disabled={turnWhy !== undefined}
              onClick={() => turn('x', 1)}
              type="button"
              data-tip={whyTip(text.turnDown, turnWhy)}
            >
              <ButtonFace icon="turnDown" label={text.turnDown} />
            </button>
            <button
              className="segment-model__button"
              disabled={turnWhy !== undefined}
              onClick={() => turn('y', -1)}
              type="button"
              data-tip={whyTip(text.turnLeft, turnWhy)}
            >
              <ButtonFace icon="turnLeft" label={text.turnLeft} />
            </button>
            <button
              className="segment-model__button"
              disabled={turnWhy !== undefined}
              onClick={() => turn('y', 1)}
              type="button"
              data-tip={whyTip(text.turnRight, turnWhy)}
            >
              <ButtonFace icon="turnRight" label={text.turnRight} />
            </button>
            <button
              className="segment-model__button"
              disabled={turnWhy !== undefined}
              onClick={() => turn('z', 1)}
              type="button"
              data-tip={whyTip(text.turnFlat, turnWhy)}
            >
              <ButtonFace icon="layFlat" label={text.turnFlat} />
            </button>
          </div>
          <CheckField checked={free} label={text.free} onChange={setFree} />
          <p className="segment-model__hint">{text.freeHint}</p>
          {free && layoutTurn !== null ? (
            <>
              {(['rotX', 'rotY', 'rotZ'] as const).map((key) => (
                <NumberField
                  key={key}
                  label={strings.model.importModel[key]}
                  onChange={(value) => setFreeFields({ ...freeFields, [key]: value })}
                  onEnter={() => applyFree(freeFields)}
                  step={1}
                  unit={text.degrees}
                  value={freeFields[key]}
                />
              ))}
            </>
          ) : null}
          <CheckField
            checked={view.undercuts}
            label={text.visualize}
            onChange={(undercuts) => store().setView({ undercuts })}
          />
        </fieldset>
      ) : null}
      <fieldset className="segment-model__group">
        <legend className="segment-model__legend">{text.options}</legend>
        <CheckField
          checked={plan.caps}
          label={text.caps}
          onChange={(caps) => setOption({ caps })}
        />
        <CheckField
          checked={plan.resizeMaterial}
          label={text.resize}
          onChange={(resizeMaterial) => setOption({ resizeMaterial })}
        />
        <CheckField
          checked={plan.boundaryVectors}
          label={text.boundary}
          onChange={(boundaryVectors) => setOption({ boundaryVectors })}
        />
      </fieldset>
      {error !== null ? (
        <p className="segment-model__error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="segment-model__actions">
        <button
          className="segment-model__button"
          onClick={() => openForm('importModel')}
          type="button"
          data-tip={text.back}
        >
          <ButtonFace icon="arrowLeft" label={text.back} />
        </button>
        <button
          className="segment-model__button segment-model__button--primary"
          disabled={actionWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.action, actionWhy)}
        >
          <ButtonFace icon="formApply" label={text.action} />
        </button>
        <button
          className="segment-model__button"
          onClick={cancel}
          type="button"
          data-tip={strings.model.importModel.cancel}
        >
          <ButtonFace icon="dismiss" label={strings.model.importModel.cancel} />
        </button>
      </div>
      {menu === null ? null : (
        <PopupMenu
          entries={treeMenu(plan, menu.id, text)}
          label={text.treeMenu(menu.id === ROOT ? text.root : text.node(menu.id))}
          onClose={() => setMenu(null)}
          onSelect={onMenu}
          x={menu.x}
          y={menu.y}
        />
      )}
    </form>
  );
}
