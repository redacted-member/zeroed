import {
  useCallback,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type JSX,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';
import { createPortal } from 'react-dom';

import type { ComponentGroup, TreeTarget } from '../../bridge/componentTree';
import type { ComponentEditOp, ComponentSummary, Level } from '../../bridge/model';
import { EDIT_OPS, editHasSettings } from '../../forms/ComponentEditForm/model';
import { menuAnchor } from '../../lib/menuAnchor';
import { reimportComponent } from '../../lib/reimport';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore, type FormId } from '../../state/uiStore';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import { saveComponentAsClip } from '../ClipartPanel/saveClip';
import { createSculptingBrush } from './createBrush';
import { TREE_COMBINES, type PropertiesTarget } from '../ComponentProperties/model';
import { RenameField } from '../RenameField/RenameField';
import { IconButton } from '../../forms/IconButton/IconButton';
import { TREE_TOOLS, type TreeTool } from './toolbar';
import { rowSpot, usePieceDrag, type TreeDrag } from './pieceDrag';
import { toolIcon } from '../../shell/ToolIcons/ToolIcons';
import { openComponentProperties, treeActions } from './actions';
import { bakeComponents, vectorBoundary } from '../../shell/MenuBar/modelMenuCommands';
import {
  buildComponentMenu,
  buildGroupMenu,
  buildLevelMenu,
  type ComponentAction,
  type GroupAction,
  type LevelAction,
} from './menu';
import {
  acceptsDrop,
  takesDrop,
  buildTree,
  dropTarget,
  listedTree,
  rangeIds,
  stepTarget,
  type DropSpot,
  type TreeItem,
} from './tree';

import './ComponentsPanel.scss';

const text = strings.model.tree;
const editText = strings.model.edit;
const EDITS = EDIT_OPS.map((op) => ({ op, label: editText[op] }));
const DRAG_TYPE = 'application/x-carve-components';

type Menu =
  | { kind: 'component'; x: number; y: number; id: number }
  | { kind: 'group'; x: number; y: number; group: ComponentGroup }
  | { kind: 'level'; x: number; y: number; level: Level; index: number };

/** Where the pointer is over a row during a drag: into a folder, or above
 * or below a component. */
type DropMark = { key: string; place: 'into' | 'before' | 'after' } | null;

function combineLabel(value: string): string {
  return TREE_COMBINES.find((c) => c.value === value)?.label ?? value;
}

/** Applies Flip at once; opens the Edit Component form on the other edits. */
function startEdit(op: ComponentEditOp, id: number): void {
  useModelStore.getState().select(id);
  if (!editHasSettings(op)) {
    void useModelStore.getState().editComponent(id, { op: 'flip' });
    return;
  }
  useModelStore.getState().setEditOp(op);
  useUiStore.getState().openForm('componentEdit');
}

/** The Component Tree: levels as folders in combine order, groups inside
 * them, components listed in the order they combine. */
export function ComponentsPanel(): JSX.Element {
  const components = useModelStore((store) => store.components);
  const levels = useModelStore((store) => store.levels);
  const activeLevel = useModelStore((store) => store.activeLevel);
  const selectedId = useModelStore((store) => store.selectedId);
  const vectorIds = useDocumentStore((store) => store.selection.ids);
  const doubleSided = useDocumentStore((store) => store.job.kind === 'doubleSided');
  const store = useModelStore.getState;
  const tree = useMemo(() => buildTree(levels, components), [levels, components]);
  const listed = useMemo(() => listedTree(tree), [tree]);

  const picked = useModelStore((store) => store.picked);
  const setPicked = useModelStore((store) => store.setPicked);
  const [groupId, setGroupId] = useState<number | null>(null);
  const [closed, setClosed] = useState<ReadonlySet<string>>(() => new Set());
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [mark, setMark] = useState<DropMark>(null);
  const dragIds = useRef<number[]>([]);
  const closeMenu = useCallback(() => setMenu(null), []);
  // While the Assembly View is open, rows drag on pointer events: within
  // the tree they reorder, onto the 3D view they become pieces.
  const levelsRef = useRef<HTMLUListElement | null>(null);
  const pressIds = useRef<number[]>([]);
  const treeDrag: TreeDrag = {
    at: (x, y) => {
      const list = levelsRef.current;
      const hit = document.elementFromPoint(x, y);
      if (list === null || hit === null || !list.contains(hit)) {
        return null;
      }
      const row = hit.closest<HTMLElement>('[data-drop-level], [data-drop-component]');
      if (row === null) {
        return { spot: null };
      }
      const box = row.getBoundingClientRect();
      const spot = rowSpot(row.dataset, box.top, box.height, y);
      const ids = pressIds.current;
      return { spot: spot !== null && takesDrop(tree, ids, spot) ? spot : null };
    },
    mark: (spot) => {
      if (spot === null) {
        setMark(null);
        return;
      }
      const key =
        spot.kind === 'component'
          ? `c${spot.id}`
          : spot.kind === 'group'
            ? `g${spot.group}`
            : `l${spot.level}`;
      const place = spot.kind === 'component' ? (spot.after ? 'before' : 'after') : 'into';
      setMark((now) => (now?.key === key && now.place === place ? now : { key, place }));
    },
    reorder: (ids, spot) => {
      const target = dropTarget(tree, ids, spot);
      if (target !== null) {
        void treeActions.move(ids, target);
      }
    },
  };
  const pieceDrag = usePieceDrag(treeDrag);
  const openProperties = (target: PropertiesTarget) => openComponentProperties(target);

  // The picked set always holds the selected component; a selection made
  // elsewhere (the 3D view) starts a fresh set.
  const pickedIds = pickedComponents({ selectedId, picked });
  const selected = components.find((c) => c.id === selectedId) ?? null;
  const groups = levels.flatMap((l) => l.groups ?? []);
  const selectedGroup = groups.find((g) => g.id === groupId) ?? null;

  const toggle = (key: string) =>
    setClosed((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });

  const pick = (id: number, event: MouseEvent | KeyboardEvent) => {
    setGroupId(null);
    if (event.shiftKey && selectedId !== null) {
      setPicked(rangeIds(listed, selectedId, id));
      store().select(id);
      return;
    }
    if (event.metaKey || event.ctrlKey) {
      const next = pickedIds.includes(id) ? pickedIds.filter((p) => p !== id) : [...pickedIds, id];
      setPicked(next);
      store().select(next.includes(id) ? id : (next[next.length - 1] ?? null));
      return;
    }
    setPicked([id]);
    store().select(id);
  };

  const pickGroup = (group: ComponentGroup) => {
    store().select(null);
    setPicked([]);
    setGroupId(group.id);
  };

  const membersOf = (id: number) => components.filter((c) => c.group === id).map((c) => c.id);

  const openForm = (form: FormId, id: number) => {
    store().select(id);
    useUiStore.getState().openForm(form);
  };

  const ungroupSelection = () => {
    const id = selectedGroup?.id ?? selected?.group ?? 0;
    if (id !== 0) {
      void treeActions.ungroup(id);
      setGroupId(null);
    }
  };

  const removeSelection = () => {
    const ids = selectedGroup === null ? pickedIds : membersOf(selectedGroup.id);
    if (ids.length > 0) {
      void treeActions.remove(ids);
      setPicked([]);
      setGroupId(null);
    }
  };

  const runComponentMenu = (action: ComponentAction, component: ComponentSummary) => {
    const id = component.id;
    const ids = pickedIds.includes(id) ? pickedIds : [id];
    switch (action.kind) {
      case 'properties':
        openProperties({ kind: 'components', ids });
        break;
      case 'rename':
        setRenamingId(id);
        break;
      case 'toggleVisible':
        void treeActions.setProps(ids, { visible: !component.visible });
        break;
      case 'combine':
        void treeActions.setProps(ids, { combine: action.combine });
        break;
      case 'level':
        void treeActions.move(ids, { level: action.level });
        break;
      case 'group':
        void treeActions.group(ids);
        break;
      case 'removeFromGroup':
        void treeActions.move(ids, { level: component.level, after: lastOfGroup(component) });
        break;
      case 'edit':
        startEdit(action.op, id);
        break;
      case 'fromSelected':
        void treeActions.fromVisible(ids);
        break;
      case 'bake':
        void bakeComponents(ids);
        break;
      case 'replaceBelow':
        openForm('replaceBelow', id);
        break;
      case 'transform':
        openForm('componentTransform', id);
        break;
      case 'saveClipart':
        void saveComponentAsClip(id, component.name);
        break;
      case 'createBrush':
        void createSculptingBrush(id);
        break;
      case 'reimport':
        void reimportComponent(id);
        break;
      case 'otherSide':
        void treeActions.toOtherSide(ids, action.copy);
        break;
      case 'delete':
        void treeActions.remove(ids);
        break;
    }
  };

  /** The last member of a component's group, so leaving the group lands
   * just after it. */
  const lastOfGroup = (component: ComponentSummary): number | undefined => {
    const members = membersOf(component.group ?? 0).filter((m) => m !== component.id);
    return members[members.length - 1];
  };

  const runGroupMenu = (action: GroupAction, group: ComponentGroup) => {
    switch (action.kind) {
      case 'properties':
        openProperties({ kind: 'group', id: group.id });
        break;
      case 'toggleVisible':
        void treeActions.updateGroup(group.id, { visible: !group.visible });
        break;
      case 'combine':
        void treeActions.updateGroup(group.id, { combine: action.combine });
        break;
      case 'ungroup':
        void treeActions.ungroup(group.id);
        break;
      case 'delete':
        void treeActions.remove(membersOf(group.id));
        break;
    }
  };

  const runLevelMenu = (action: LevelAction, level: Level) => {
    switch (action.kind) {
      case 'properties':
        openProperties({ kind: 'level', id: level.id });
        break;
      case 'toggleVisible':
        void store().updateLevel(level.id, { visible: !level.visible });
        break;
      case 'combine':
        void store().updateLevel(level.id, { combine: action.combine });
        break;
      case 'makeActive':
        void store().setActiveLevel(level.id);
        break;
      case 'clip':
        void treeActions.setClip(level.id, vectorIds);
        break;
      case 'updateClip':
        void treeActions.updateClip(level.id, vectorIds);
        break;
      case 'clearClip':
        void treeActions.setClip(level.id, []);
        break;
      case 'mirror':
        void store().updateLevel(level.id, { mirror: action.mirror });
        break;
      case 'up':
        void store().moveLevel(level.id, 1);
        break;
      case 'down':
        void store().moveLevel(level.id, -1);
        break;
      case 'newLevel':
        void store().addLevel();
        break;
      case 'fromVisible':
        void treeActions.fromVisible();
        break;
      case 'boundary':
        void vectorBoundary();
        break;
      case 'exportModel':
        useUiStore.getState().openForm('exportStl');
        break;
      case 'delete':
        void store().deleteLevel(level.id);
        break;
    }
  };

  // ------------------------------------------------------------ dragging

  const startDrag = (event: DragEvent, ids: number[]) => {
    dragIds.current = ids;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(DRAG_TYPE, JSON.stringify(ids));
  };

  const endDrag = () => {
    dragIds.current = [];
    setMark(null);
  };

  const spotOf = (event: DragEvent<HTMLElement>, id: number): DropSpot => {
    const box = event.currentTarget.getBoundingClientRect();
    // The list runs bottom up: the upper half of a row is later in combine
    // order.
    return { kind: 'component', id, after: event.clientY < box.top + box.height / 2 };
  };

  const over = (event: DragEvent<HTMLElement>, key: string, spot: DropSpot, allowed: boolean) => {
    if (!allowed || dragIds.current.length === 0) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = 'move';
    const place = spot.kind === 'component' ? (spot.after ? 'before' : 'after') : 'into';
    if (mark?.key !== key || mark.place !== place) {
      setMark({ key, place });
    }
  };

  const drop = (event: DragEvent<HTMLElement>, spot: DropSpot) => {
    event.preventDefault();
    event.stopPropagation();
    const ids = dragIds.current;
    endDrag();
    const target: TreeTarget | null = dropTarget(tree, ids, spot);
    if (target !== null) {
      void treeActions.move(ids, target);
    }
  };

  const markClass = (key: string, base: string) => {
    if (mark?.key !== key) {
      return base;
    }
    return `${base} components-panel__drop--${mark.place}`;
  };

  // ------------------------------------------------------------ rows

  /** Moves a component up (1) or down (-1) the list, one combine step. */
  const step = (id: number, direction: -1 | 1) => {
    const target = stepTarget(tree, id, direction);
    if (target !== null) {
      void treeActions.move([id], target);
    }
  };

  const rowKeys = (event: KeyboardEvent<HTMLButtonElement>, component: ComponentSummary) => {
    if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      event.preventDefault();
      step(component.id, event.key === 'ArrowUp' ? 1 : -1);
    } else if (event.key === 'F2') {
      event.preventDefault();
      setRenamingId(component.id);
    } else if (event.key === 'Enter' && event.altKey) {
      event.preventDefault();
      openProperties({ kind: 'components', ids: pickedIds });
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      removeSelection();
    }
  };

  const componentRow = (component: ComponentSummary, nested: boolean) => {
    const key = `c${component.id}`;
    const isPicked = pickedIds.includes(component.id);
    return (
      <li
        className={markClass(
          key,
          [
            'components-panel__row',
            nested ? 'components-panel__row--nested' : '',
            isPicked ? 'components-panel__row--selected' : '',
            pieceDrag.active ? 'components-panel__row--piece' : '',
          ]
            .filter((c) => c !== '')
            .join(' '),
        )}
        data-drop-component={component.id}
        draggable={renamingId !== component.id && !pieceDrag.active}
        key={component.id}
        onPointerDown={(event) => {
          const ids = isPicked ? pickedIds : [component.id];
          pressIds.current = ids;
          pieceDrag.press(event, component.id, component.name, ids);
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          if (!isPicked) {
            setPicked([component.id]);
          }
          store().select(component.id);
          setGroupId(null);
          setMenu({ kind: 'component', ...menuAnchor(event), id: component.id });
        }}
        onDragEnd={endDrag}
        onDragLeave={() => setMark(null)}
        onDragOver={(event) =>
          over(event, key, spotOf(event, component.id), !dragIds.current.includes(component.id))
        }
        onDragStart={(event) => startDrag(event, isPicked ? pickedIds : [component.id])}
        onDrop={(event) => drop(event, spotOf(event, component.id))}
      >
        <input
          aria-label={text.visible(component.name)}
          checked={component.visible}
          className="components-panel__check"
          onChange={(event) =>
            void treeActions.setProps([component.id], { visible: event.target.checked })
          }
          type="checkbox"
        />
        {component.color === null || component.color === undefined ? null : (
          <span aria-hidden="true" className="components-panel__swatch">
            <svg className="components-panel__swatch-dot" viewBox="0 0 10 10">
              <circle cx="5" cy="5" fill={component.color} r="4" />
            </svg>
          </span>
        )}
        {renamingId === component.id ? (
          <RenameField
            label={strings.panelMenus.renameField(component.name)}
            name={component.name}
            onDone={(name) => {
              setRenamingId(null);
              if (name !== null) {
                void treeActions.setProps([component.id], { name });
              }
            }}
          />
        ) : (
          <button
            aria-pressed={isPicked}
            className="components-panel__name"
            data-tip={pieceDrag.active ? strings.assembly.treeDragRow(component.name) : undefined}
            onClick={(event) => pick(component.id, event)}
            onDoubleClick={() =>
              openProperties({
                kind: 'components',
                ids: isPicked ? pickedIds : [component.id],
              })
            }
            onKeyDown={(event) => rowKeys(event, component)}
            type="button"
          >
            <span className="components-panel__label">{component.name}</span>
          </button>
        )}
        <span
          className="components-panel__badge"
          data-tip={text.combineBadge(combineLabel(component.combine))}
        >
          {combineLabel(component.combine)}
        </span>
      </li>
    );
  };

  const groupRow = (node: { level: Level }, item: Extract<TreeItem, { kind: 'group' }>) => {
    const { group, members } = item;
    const key = `g${group.id}`;
    const open = !closed.has(key);
    const ids = members.map((m) => m.id);
    const spot: DropSpot = { kind: 'group', level: node.level.id, group: group.id };
    return (
      <li className="components-panel__folder" key={key}>
        <div
          className={markClass(
            key,
            group.id === groupId
              ? 'components-panel__group-row components-panel__row--selected'
              : 'components-panel__group-row',
          )}
          data-drop-group={group.id}
          data-drop-level={node.level.id}
          draggable
          onContextMenu={(event) => {
            event.preventDefault();
            pickGroup(group);
            setMenu({ kind: 'group', ...menuAnchor(event), group });
          }}
          onDragEnd={endDrag}
          onDragLeave={() => setMark(null)}
          onDragOver={(event) => over(event, key, spot, acceptsDrop(item, dragIds.current))}
          onDragStart={(event) => startDrag(event, ids)}
          onDrop={(event) => drop(event, spot)}
        >
          <button
            aria-expanded={open}
            aria-label={open ? text.collapse(group.name) : text.expand(group.name)}
            data-tip={open ? text.collapse(group.name) : text.expand(group.name)}
            className="components-panel__twisty"
            onClick={() => toggle(key)}
            type="button"
          >
            {open ? '▾' : '▸'}
          </button>
          <input
            aria-label={text.groupVisible(group.name)}
            checked={group.visible}
            className="components-panel__check"
            onChange={(event) =>
              void treeActions.updateGroup(group.id, { visible: event.target.checked })
            }
            type="checkbox"
          />
          <button
            aria-pressed={group.id === groupId}
            className="components-panel__name components-panel__name--folder"
            onClick={() => pickGroup(group)}
            onDoubleClick={() => openProperties({ kind: 'group', id: group.id })}
            type="button"
          >
            <span className="components-panel__label">{group.name}</span>
          </button>
          <span
            className="components-panel__badge"
            data-tip={text.combineBadge(combineLabel(group.combine))}
          >
            {combineLabel(group.combine)}
          </span>
        </div>
        {open ? (
          <ul aria-label={group.name} className="components-panel__list">
            {members.map((member) => componentRow(member, true))}
          </ul>
        ) : null}
      </li>
    );
  };

  const noPick = pickedIds.length === 0 && selectedGroup === null;
  const why = strings.why;
  const stepWhy = (by: 1 | -1, end: string): string | undefined =>
    selectedId === null
      ? why.pickComponent
      : stepTarget(tree, selectedId, by) === null
        ? end
        : undefined;
  const toolWhy: Record<TreeTool, string | undefined> = {
    moveUp: stepWhy(1, why.atTop),
    moveDown: stepWhy(-1, why.atBottom),
    newLevel: undefined,
    group: pickedIds.length === 0 ? why.pickComponent : undefined,
    ungroup: selectedGroup === null && (selected?.group ?? 0) === 0 ? why.pickGroup : undefined,
    fromVisible: components.length === 0 ? why.needsModel : undefined,
    fromVectors: undefined,
    properties: noPick ? why.pickComponent : undefined,
    remove: noPick ? why.pickComponent : undefined,
  };
  const toolActions: Record<TreeTool, () => void> = {
    moveUp: () => selectedId !== null && step(selectedId, 1),
    moveDown: () => selectedId !== null && step(selectedId, -1),
    newLevel: () => void store().addLevel(),
    group: () => void treeActions.group(pickedIds),
    ungroup: ungroupSelection,
    fromVisible: () => void treeActions.fromVisible(),
    fromVectors: () => useUiStore.getState().openForm('createShape'),
    properties: () =>
      openProperties(
        selectedGroup === null
          ? { kind: 'components', ids: pickedIds }
          : { kind: 'group', id: selectedGroup.id },
      ),
    remove: removeSelection,
  };

  const empty = components.length === 0 && levels.length <= 1;

  return (
    <div className="components-panel">
      <div aria-label={text.tools} className="components-panel__toolbar" role="toolbar">
        {TREE_TOOLS.map((tool) => (
          <IconButton
            why={toolWhy[tool.id]}
            icon={tool.icon}
            key={tool.id}
            label={tool.label}
            onClick={toolActions[tool.id]}
            title={tool.title}
          />
        ))}
        {pickedIds.length > 1 ? (
          <span className="components-panel__picked">{text.picked(pickedIds.length)}</span>
        ) : null}
      </div>
      {pieceDrag.active && components.length > 0 ? (
        <p className="components-panel__piece-hint">
          <span aria-hidden="true" className="components-panel__piece-hint-icon">
            {toolIcon('pieceDrag')}
          </span>
          {strings.assembly.treeDragHint}
        </p>
      ) : null}
      {pieceDrag.label === null
        ? null
        : createPortal(
            <div
              aria-hidden="true"
              className={
                pieceDrag.label.over
                  ? 'components-panel__drag-label components-panel__drag-label--over'
                  : 'components-panel__drag-label'
              }
              style={{ left: pieceDrag.label.x, top: pieceDrag.label.y }}
            >
              {pieceDrag.label.name}
            </div>,
            document.body,
          )}
      {empty ? (
        <p className="components-panel__empty">{text.empty}</p>
      ) : (
        <ul aria-label={text.label} className="components-panel__levels" ref={levelsRef}>
          {listed.map((node) => {
            const { level, items } = node;
            const index = levels.findIndex((l) => l.id === level.id);
            const key = `l${level.id}`;
            const open = !closed.has(key);
            const spot: DropSpot = { kind: 'level', level: level.id };
            const clipped = (level.clip ?? []).length > 0;
            return (
              <li className="components-panel__folder" key={key}>
                <div
                  className={markClass(
                    key,
                    level.id === activeLevel
                      ? 'components-panel__level-row components-panel__level-row--active'
                      : 'components-panel__level-row',
                  )}
                  data-drop-level={level.id}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    setMenu({ kind: 'level', ...menuAnchor(event), level, index });
                  }}
                  onDragLeave={() => setMark(null)}
                  onDragOver={(event) => over(event, key, spot, true)}
                  onDrop={(event) => drop(event, spot)}
                >
                  <button
                    aria-expanded={open}
                    aria-label={open ? text.collapse(level.name) : text.expand(level.name)}
                    data-tip={open ? text.collapse(level.name) : text.expand(level.name)}
                    className="components-panel__twisty"
                    onClick={() => toggle(key)}
                    type="button"
                  >
                    {open ? '▾' : '▸'}
                  </button>
                  <input
                    aria-label={text.levelVisible(level.name)}
                    checked={level.visible}
                    className="components-panel__check"
                    onChange={(event) =>
                      void store().updateLevel(level.id, { visible: event.target.checked })
                    }
                    type="checkbox"
                  />
                  <button
                    aria-current={level.id === activeLevel ? 'true' : undefined}
                    className="components-panel__name components-panel__name--folder"
                    onClick={() => void store().setActiveLevel(level.id)}
                    onDoubleClick={() => openProperties({ kind: 'level', id: level.id })}
                    data-tip={
                      level.id === activeLevel
                        ? strings.model.components.levelActive(level.name)
                        : strings.model.components.makeActive(level.name)
                    }
                    type="button"
                  >
                    <span className="components-panel__label">{level.name}</span>
                  </button>
                  {clipped ? <span className="components-panel__badge">{text.clipped}</span> : null}
                  {(level.mirror ?? null) === null ? null : (
                    <span className="components-panel__badge">{text.mirrored}</span>
                  )}
                  <span
                    className="components-panel__badge"
                    data-tip={text.combineBadge(combineLabel(level.combine))}
                  >
                    {combineLabel(level.combine)}
                  </span>
                </div>
                {!open ? null : items.length === 0 ? (
                  <p className="components-panel__empty components-panel__empty--level">
                    {text.emptyLevel}
                  </p>
                ) : (
                  <ul aria-label={level.name} className="components-panel__list">
                    {items.map((item) =>
                      item.kind === 'component'
                        ? componentRow(item.component, false)
                        : groupRow(node, item),
                    )}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {menu?.kind === 'component'
        ? (() => {
            const component = components.find((c) => c.id === menu.id);
            return component === undefined ? null : (
              <PopupMenu
                entries={buildComponentMenu({
                  component,
                  picked: pickedIds.includes(component.id) ? pickedIds.length : 1,
                  levels,
                  combines: TREE_COMBINES,
                  edits: EDITS,
                  doubleSided,
                })}
                label={strings.panelMenus.componentLabel(component.name)}
                onClose={closeMenu}
                onSelect={(action) => runComponentMenu(action, component)}
                x={menu.x}
                y={menu.y}
              />
            );
          })()
        : null}
      {menu?.kind === 'group' ? (
        <PopupMenu
          entries={buildGroupMenu(menu.group, TREE_COMBINES)}
          label={strings.model.treeMenu.groupLabel(menu.group.name)}
          onClose={closeMenu}
          onSelect={(action) => runGroupMenu(action, menu.group)}
          x={menu.x}
          y={menu.y}
        />
      ) : null}
      {menu?.kind === 'level' ? (
        <PopupMenu
          entries={buildLevelMenu({
            level: menu.level,
            index: menu.index,
            count: levels.length,
            active: menu.level.id === activeLevel,
            vectorsSelected: vectorIds.length > 0,
            combines: TREE_COMBINES,
          })}
          label={strings.model.treeMenu.levelLabel(menu.level.name)}
          onClose={closeMenu}
          onSelect={(action) => runLevelMenu(action, menu.level)}
          x={menu.x}
          y={menu.y}
        />
      ) : null}
    </div>
  );
}
