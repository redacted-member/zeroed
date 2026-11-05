import { useCallback, useId, useMemo, useState, type DragEvent, type JSX } from 'react';

import { getKernel } from '../../bridge/client';
import type { ToolpathGroup, ToolpathSummary } from '../../bridge/cam';
import { menuAnchor } from '../../lib/menuAnchor';
import { strings } from '../../lib/strings';
import { whyStrings } from '../../lib/strings/why';
import { moveWhy } from '../../forms/ButtonFace/why';
import { whyTip } from '../../shell/Tooltip/model';
import { isDoubleSided, sideLabel, toolpathSideTag } from '../../lib/sides';
import { formatDuration } from '../../lib/toolpathFields';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { usePreviewStore } from '../../state/previewStore';
import { toolpathColour } from '../../forms/PreviewToolpathsForm/model';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import { RenameField } from '../RenameField/RenameField';
import {
  buildGroupMenu,
  buildListMenu,
  buildToolpathMenu,
  type GroupAction,
  type ListAction,
  type ToolpathAction,
} from './menu';
import {
  buildTree,
  dropPlace,
  groupVisibility,
  shownToolpaths,
  statusOf,
  type SheetFilter,
  type TreeGroup,
  type TreeNode,
  type TreeToolpath,
  onSide,
  initialSheetFilter,
} from './tree';

import { duplicateToolpath, openToolpathEditor } from './actions';
import { formForToolpath } from './editForm';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import './ToolpathsPanel.scss';

const text = strings.toolpathShell.list;

/** A small drawing of the tool's shape, side on. */
function ToolIcon({ type }: { type: string | undefined }): JSX.Element {
  let shape: JSX.Element;
  switch (type) {
    case 'ballNose':
    case 'taperedBallNose':
      shape = <path d="M4 1 H10 V9 A3 3 0 0 1 4 9 Z" />;
      break;
    case 'vBit':
    case 'engravingTool':
    case 'diamondDrag':
      shape = <path d="M3 1 H11 V6 L7 13 L3 6 Z" />;
      break;
    case 'drill':
      shape = <path d="M4 1 H10 V10 L7 13 L4 10 Z" />;
      break;
    case 'laserTool':
      shape = <path d="M5 1 H9 V7 L7 13 L5 7 Z" />;
      break;
    default:
      shape = <path d="M4 1 H10 V12 H4 Z" />;
  }
  return (
    <svg
      aria-label={text.toolIcon(type ?? '')}
      className="toolpaths-panel__tool"
      role="img"
      viewBox="0 0 14 14"
    >
      {shape}
    </svg>
  );
}

type Menu =
  | { kind: 'toolpath'; x: number; y: number; id: number }
  | { kind: 'group'; x: number; y: number; id: number }
  | { kind: 'list'; x: number; y: number };

/** The toolpath list in the right panel. */
export function ToolpathsPanel(): JSX.Element {
  // Menu, form and panel icons load after start up; redraw once they are in.
  useExtraIcons();
  const everyToolpath = useToolpathStore((store) => store.toolpaths);
  const job = useDocumentStore((state) => state.job);
  const activeSide = useDocumentStore((state) => state.activeSide);
  const allToolpaths = useMemo(
    () => onSide(everyToolpath, activeSide, isDoubleSided(job)),
    [activeSide, everyToolpath, job],
  );
  const groups = useToolpathStore((store) => store.groups);
  const sheets = useDocumentStore((store) => store.sheets);
  const activeSheet = sheets.find((s) => s.active)?.id ?? 0;
  const setActiveSheet = useDocumentStore((store) => store.setActiveSheet);
  const [filterChoice, setFilterChoice] = useState<SheetFilter | null>(() =>
    initialSheetFilter(usePrefsStore.getState().prefs.toolpaths.defaultFilter),
  );
  // Following the active sheet until a sheet or All Sheets is picked; a
  // picked sheet that went away follows the active one again.
  const filter: SheetFilter =
    filterChoice === null ||
    (typeof filterChoice === 'number' && !sheets.some((s) => s.id === filterChoice))
      ? activeSheet
      : filterChoice;
  const tree = useMemo(
    () => buildTree(allToolpaths, groups, filter, activeSheet),
    [activeSheet, allToolpaths, filter, groups],
  );
  const toolpaths = useMemo(() => shownToolpaths(tree), [tree]);
  const selectedId = useToolpathStore((store) => store.selectedId);
  // Preview Toolpaths with Toolpath Color: a square of each toolpath's fill
  // color beside its name.
  const toolpathFill = usePreviewStore((store) => store.fill === 'toolpath' && !store.lithophane);
  const previewOpen = useUiStore((store) => store.activeForm === 'previewToolpaths');
  const swatches = toolpathFill && previewOpen;
  const chosenColours = usePreviewStore((store) => store.toolpathColours);
  const selectedGroupId = useToolpathStore((store) => store.selectedGroupId);
  const store = useToolpathStore.getState;
  const openForm = useUiStore((ui) => ui.openForm);
  const selected = toolpaths.find((t) => t.id === selectedId) ?? null;
  const selectedIndex = selected === null ? -1 : toolpaths.indexOf(selected);
  const upWhy =
    selected === null ? whyStrings.why.pickToolpath : moveWhy(selectedIndex, toolpaths.length, -1);
  const downWhy =
    selected === null ? whyStrings.why.pickToolpath : moveWhy(selectedIndex, toolpaths.length, 1);
  const total = toolpaths.filter((t) => t.visible).reduce((sum, t) => sum + t.estimatedSeconds, 0);
  const filterId = useId();

  const swatch = (toolpath: ToolpathSummary) => {
    const position = everyToolpath.findIndex((t) => t.id === toolpath.id);
    const colour = toolpathColour(Math.max(position, 0), toolpath.id, chosenColours);
    return colour === 'none' ? (
      <span
        className="toolpaths-panel__swatch toolpaths-panel__swatch--none"
        data-tip={strings.toolpathShell.list.noFill}
      />
    ) : (
      // The square shows the color the user picked, a computed value.
      <span
        className="toolpaths-panel__swatch"
        style={{ backgroundColor: colour }}
        data-tip={colour}
      />
    );
  };

  const sideTag = (toolpath: ToolpathSummary) => {
    const tag = toolpathSideTag(toolpath.side, job);
    return tag === null ? null : (
      <span
        className={`toolpaths-panel__side toolpaths-panel__side--${toolpath.side ?? 'top'}`}
        data-tip={strings.sides.tag(tag)}
      >
        {tag}
      </span>
    );
  };

  const edit = openToolpathEditor;

  const [renaming, setRenaming] = useState<{ kind: 'toolpath' | 'group'; id: number } | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [dragged, setDragged] = useState<number | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);

  const visibleIds = toolpaths.filter((t) => t.visible).map((t) => t.id);
  const ids = (list: readonly ToolpathSummary[]) => list.map((t) => t.id);

  const deleteGroup = async (group: ToolpathGroup) => {
    const alsoToolpaths = await getKernel().confirm(text.deleteGroupToolpaths(group.name));
    await store().deleteGroup(group.id, alsoToolpaths);
  };

  const runToolpathMenu = (toolpath: ToolpathSummary, action: ToolpathAction) => {
    const id = toolpath.id;
    const others = toolpaths.filter((t) => t.id !== id);
    if (typeof action === 'object') {
      void store().setSheet([id], action.moveToSheet);
      return;
    }
    switch (action) {
      case 'showThis':
      case 'hideThis':
        void store().setVisible(id, action === 'showThis');
        break;
      case 'showOnly':
        void store()
          .setVisibleMany(ids(others), false)
          .then(() => store().setVisible(id, true));
        break;
      case 'showAllBut':
        void store()
          .setVisibleMany(ids(others), true)
          .then(() => store().setVisible(id, false));
        break;
      case 'showSameTool':
        void store().setVisibleMany(
          ids(toolpaths.filter((t) => t.toolName === toolpath.toolName)),
          true,
        );
        break;
      case 'showAll':
        void store().setVisibleMany(ids(toolpaths), true);
        break;
      case 'hideAll':
        void store().setVisibleMany(ids(toolpaths), false);
        break;
      case 'activateSheet':
        if (toolpath.sheet !== undefined && toolpath.sheet !== 0) {
          void setActiveSheet(toolpath.sheet);
        }
        break;
      case 'edit':
        edit(toolpath);
        break;
      case 'rename':
        setRenaming({ kind: 'toolpath', id });
        break;
      case 'duplicate':
        void duplicateToolpath(toolpath);
        break;
      case 'recalculate':
        void store().recalculate(id);
        break;
      case 'recalculateVisible':
        void store().recalculateMany(visibleIds);
        break;
      case 'recalculateAll':
        void store().recalculateMany(ids(allToolpaths));
        break;
      case 'createGroup':
        void store().createGroup([]);
        break;
      case 'groupVisible':
        void store().createGroup(visibleIds);
        break;
      case 'ungroup':
        void store().place([id], 0);
        break;
      case 'save':
        store().select(id);
        store().setSaveChoice([id]);
        openForm('saveToolpaths');
        break;
      case 'delete':
        void store().remove(id);
        break;
      case 'deleteInvisible':
        void store().removeMany(ids(toolpaths.filter((t) => !t.visible)));
        break;
      case 'deleteVisible':
        void store().removeMany(visibleIds);
        break;
      case 'deleteAll':
        void store().removeMany(ids(toolpaths));
        break;
    }
  };

  const runGroupMenu = (node: TreeGroup, action: GroupAction) => {
    const group = node.group;
    const members = node.members.map((m) => m.toolpath.id);
    switch (action) {
      case 'rename':
        setRenaming({ kind: 'group', id: group.id });
        break;
      case 'expand':
        void store().updateGroup(group.id, { expanded: !group.expanded });
        break;
      case 'showGroup':
      case 'hideGroup':
        void store().setVisibleMany(members, action === 'showGroup');
        break;
      case 'recalculateGroup':
        void store().recalculateMany(members);
        break;
      case 'ungroup':
        void store().deleteGroup(group.id, false);
        break;
      case 'deleteGroup':
        void deleteGroup(group);
        break;
      case 'createGroup':
        void store().createGroup([]);
        break;
      case 'groupVisible':
        void store().createGroup(visibleIds);
        break;
    }
  };

  const runListMenu = (action: ListAction) => {
    void store().createGroup(action === 'groupVisible' ? visibleIds : []);
  };

  const onDragOver = (key: string) => (event: DragEvent) => {
    if (dragged === null) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    setDropTarget(key);
  };

  const onDrop = (target: TreeNode | null) => (event: DragEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setDropTarget(null);
    const id = dragged;
    setDragged(null);
    if (id === null) {
      return;
    }
    const place = dropPlace(id, target);
    if (place !== null) {
      void store().place([id], place.group, place.before);
    }
  };

  const toolpathRow = (node: TreeToolpath) => {
    const toolpath = node.toolpath;
    const status = statusOf(toolpath);
    const key = `t${toolpath.id}`;
    const classes = ['toolpaths-panel__row'];
    if (toolpath.id === selectedId) {
      classes.push('toolpaths-panel__row--selected');
    }
    if (node.inGroup) {
      classes.push('toolpaths-panel__row--member');
    }
    if (node.faded) {
      classes.push('toolpaths-panel__row--faded');
    }
    if (dropTarget === key) {
      classes.push('toolpaths-panel__row--drop');
    }
    return (
      <li
        className={classes.join(' ')}
        draggable
        key={key}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          store().select(toolpath.id);
          setMenu({ kind: 'toolpath', ...menuAnchor(event), id: toolpath.id });
        }}
        onDragEnd={() => {
          setDragged(null);
          setDropTarget(null);
        }}
        onDragOver={onDragOver(key)}
        onDragStart={(event) => {
          setDragged(toolpath.id);
          event.dataTransfer.effectAllowed = 'move';
        }}
        onDrop={onDrop(node)}
      >
        <input
          aria-label={text.visible(toolpath.name)}
          checked={toolpath.visible}
          className="toolpaths-panel__visible"
          onChange={(event) => void store().setVisible(toolpath.id, event.target.checked)}
          type="checkbox"
        />
        <ToolIcon type={toolpath.toolType} />
        {renaming?.kind === 'toolpath' && renaming.id === toolpath.id ? (
          <RenameField
            label={strings.panelMenus.renameField(toolpath.name)}
            name={toolpath.name}
            onDone={(name) => {
              setRenaming(null);
              if (name !== null) {
                void store().rename(toolpath.id, name);
              }
            }}
          />
        ) : (
          <button
            aria-pressed={toolpath.id === selectedId}
            className="toolpaths-panel__name"
            onClick={() => store().select(toolpath.id)}
            onDoubleClick={() => edit(toolpath)}
            data-tip={`${toolpath.toolName}. ${text.time(formatDuration(toolpath.estimatedSeconds))}`}
            type="button"
          >
            <span className="toolpaths-panel__label">{toolpath.name}</span>
            {swatches ? swatch(toolpath) : null}
            {sideTag(toolpath)}
            <span
              aria-label={
                status === 'stale'
                  ? text.statusStale
                  : status === 'empty'
                    ? text.statusEmpty
                    : text.statusOk
              }
              className={`toolpaths-panel__status toolpaths-panel__status--${status}`}
              role="img"
              data-tip={
                status === 'stale'
                  ? text.statusStale
                  : status === 'empty'
                    ? text.statusEmpty
                    : text.statusOk
              }
            />
            <span className="toolpaths-panel__time">
              {formatDuration(toolpath.estimatedSeconds)}
            </span>
          </button>
        )}
      </li>
    );
  };

  const groupRow = (node: TreeGroup) => {
    const group = node.group;
    const key = `g${group.id}`;
    const shown = groupVisibility(node);
    const classes = ['toolpaths-panel__row', 'toolpaths-panel__row--group'];
    if (group.id === selectedGroupId) {
      classes.push('toolpaths-panel__row--selected');
    }
    if (node.faded) {
      classes.push('toolpaths-panel__row--faded');
    }
    if (dropTarget === key) {
      classes.push('toolpaths-panel__row--drop');
    }
    return (
      <li className="toolpaths-panel__group" key={key}>
        <div
          className={classes.join(' ')}
          onContextMenu={(event) => {
            event.preventDefault();
            event.stopPropagation();
            setMenu({ kind: 'group', ...menuAnchor(event), id: group.id });
          }}
          onDragOver={onDragOver(key)}
          onDrop={onDrop(node)}
        >
          <button
            aria-expanded={group.expanded}
            aria-label={text.toggleGroup(group.name)}
            data-tip={text.toggleGroup(group.name)}
            className="toolpaths-panel__twisty"
            onClick={() => void store().updateGroup(group.id, { expanded: !group.expanded })}
            type="button"
          >
            {group.expanded ? '▾' : '▸'}
          </button>
          <input
            aria-label={text.groupVisibleLabel(group.name)}
            checked={shown === 'all'}
            className="toolpaths-panel__visible"
            disabled={node.members.length === 0}
            onChange={(event) =>
              void store().setVisibleMany(
                node.members.map((m) => m.toolpath.id),
                event.target.checked,
              )
            }
            ref={(input) => {
              if (input !== null) {
                input.indeterminate = shown === 'some';
              }
            }}
            type="checkbox"
          />
          {renaming?.kind === 'group' && renaming.id === group.id ? (
            <RenameField
              label={strings.panelMenus.renameField(group.name)}
              name={group.name}
              onDone={(name) => {
                setRenaming(null);
                if (name !== null) {
                  void store().updateGroup(group.id, { name });
                }
              }}
            />
          ) : (
            <button
              aria-pressed={group.id === selectedGroupId}
              className="toolpaths-panel__group-name"
              onClick={() => store().selectGroup(group.id)}
              onDoubleClick={() => setRenaming({ kind: 'group', id: group.id })}
              type="button"
            >
              {group.name}
            </button>
          )}
        </div>
        {group.expanded && node.members.length > 0 ? (
          <ul className="toolpaths-panel__members">{node.members.map(toolpathRow)}</ul>
        ) : null}
      </li>
    );
  };

  const menuToolpath =
    menu?.kind === 'toolpath' ? (toolpaths.find((t) => t.id === menu.id) ?? null) : null;
  const menuGroup =
    menu?.kind === 'group'
      ? (tree.find((n): n is TreeGroup => n.kind === 'group' && n.group.id === menu.id) ?? null)
      : null;

  return (
    <div className="toolpaths-panel">
      <div className="toolpaths-panel__header">
        <input
          aria-label={text.allVisible}
          checked={toolpaths.length > 0 && visibleIds.length === toolpaths.length}
          className="toolpaths-panel__visible"
          disabled={toolpaths.length === 0}
          onChange={(event) => void store().setVisibleMany(ids(toolpaths), event.target.checked)}
          ref={(input) => {
            if (input !== null) {
              input.indeterminate = visibleIds.length > 0 && visibleIds.length < toolpaths.length;
            }
          }}
          type="checkbox"
        />
        <h2 className="toolpaths-panel__title" id="toolpath-list-title">
          {isDoubleSided(job) ? text.headingSide(sideLabel(activeSide)) : text.heading}
        </h2>
        <label className="toolpaths-panel__sr-only" htmlFor={filterId}>
          {text.sheetFilter}
        </label>
        <select
          className="toolpaths-panel__select"
          id={filterId}
          onChange={(event) => {
            // Picking a sheet makes it active; All Sheets leaves it be.
            const picked = event.target.value === 'all' ? 'all' : Number(event.target.value);
            setFilterChoice(picked);
            if (picked !== 'all') {
              void setActiveSheet(picked);
            }
          }}
          data-tip={text.sheetFilter}
          value={String(filter)}
        >
          <option value="all">{text.allSheets}</option>
          {sheets.map((sheet) => (
            <option key={sheet.id} value={String(sheet.id)}>
              {sheet.name}
            </option>
          ))}
        </select>
        <button
          aria-label={text.up}
          className="toolpaths-panel__arrow"
          disabled={upWhy !== undefined}
          onClick={() => selected !== null && void store().move(selected.id, -1)}
          data-tip={whyTip(text.up, upWhy)}
          type="button"
        >
          {toolIcon('arrowUp')}
        </button>
        <button
          aria-label={text.down}
          className="toolpaths-panel__arrow"
          disabled={downWhy !== undefined}
          onClick={() => selected !== null && void store().move(selected.id, 1)}
          data-tip={whyTip(text.down, downWhy)}
          type="button"
        >
          {toolIcon('arrowDown')}
        </button>
      </div>
      {tree.length === 0 ? (
        <p
          className="toolpaths-panel__empty"
          onContextMenu={(event) => {
            event.preventDefault();
            setMenu({ kind: 'list', ...menuAnchor(event) });
          }}
        >
          {text.empty}
        </p>
      ) : (
        <ul
          aria-label={text.label}
          className={
            dropTarget === 'list'
              ? 'toolpaths-panel__list toolpaths-panel__list--drop'
              : 'toolpaths-panel__list'
          }
          onContextMenu={(event) => {
            event.preventDefault();
            setMenu({ kind: 'list', ...menuAnchor(event) });
          }}
          onDragOver={onDragOver('list')}
          onDrop={onDrop(null)}
        >
          {tree.map((node) => (node.kind === 'group' ? groupRow(node) : toolpathRow(node)))}
        </ul>
      )}
      {toolpaths.length === 0 ? null : (
        <div className="toolpaths-panel__total">
          <span>{text.time(formatDuration(total))}</span>
          <button
            className="toolpaths-panel__estimate"
            onClick={() => useDialogStore.getState().show('machiningTime')}
            type="button"
            data-tip={text.estimateButton}
          >
            <ButtonFace icon="toolpathSummary" label={text.estimateButton} />
          </button>
        </div>
      )}
      {menu?.kind === 'toolpath' && menuToolpath !== null ? (
        <PopupMenu
          entries={buildToolpathMenu({
            toolpath: menuToolpath,
            canEdit: formForToolpath(menuToolpath) !== null,
            activeSheet,
            sheets: sheets.map((s) => ({ id: s.id, name: s.name })),
          })}
          label={strings.panelMenus.toolpathLabel(menuToolpath.name)}
          onClose={closeMenu}
          onSelect={(action) => runToolpathMenu(menuToolpath, action)}
          x={menu.x}
          y={menu.y}
        />
      ) : null}
      {menu?.kind === 'group' && menuGroup !== null ? (
        <PopupMenu
          entries={buildGroupMenu(menuGroup.group, menuGroup.members.length)}
          label={text.groupLabel(menuGroup.group.name)}
          onClose={closeMenu}
          onSelect={(action) => runGroupMenu(menuGroup, action)}
          x={menu.x}
          y={menu.y}
        />
      ) : null}
      {menu?.kind === 'list' ? (
        <PopupMenu
          entries={buildListMenu(visibleIds.length > 0)}
          label={text.listLabel}
          onClose={closeMenu}
          onSelect={runListMenu}
          x={menu.x}
          y={menu.y}
        />
      ) : null}
    </div>
  );
}
