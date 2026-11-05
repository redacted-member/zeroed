import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type JSX,
  type KeyboardEvent,
} from 'react';

import { api } from '../../bridge/formApi';
import type {
  NameFormats,
  Tool,
  ToolDatabaseList,
  ToolImportMode,
  ToolKind,
} from '../../bridge/cam';
import { toolForToolpath } from '../../bridge/cam';
import { getKernel } from '../../bridge/client';
import { reloadTools, useToolDatabase } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { useDialogStore } from '../../state/dialogStore';
import { confirmationOn, usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { ToolEditor } from '../ToolEditor/ToolEditor';
import {
  EDITABLE_KINDS,
  blankTool,
  breadcrumb,
  groupEditorName,
  nudge,
  pickableDatabase,
  placement,
  type Placement,
  type TreeItem,
} from './model';

import './ToolDatabase.scss';
import { dialogKeyDown } from '../../lib/dialogKeys';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

const text = strings.toolDatabase;
const ROOT = 0;
const TOOL_DB_FILTER = [{ name: text.fileFilter, extensions: ['json'] }];
const IMPORT_MODES: readonly ToolImportMode[] = ['import', 'mergeOverwrite', 'mergeKeep'];

type Picked = TreeItem | null;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The tool database, as a modal dialog over the window. */
export function ToolDatabase(): JSX.Element | null {
  const open = useUiStore((ui) => ui.toolDatabaseOpen);
  const close = useUiStore((ui) => ui.closeToolDatabase);
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
      aria-labelledby="tool-database-title"
      className="tool-database"
      onClose={close}
      onKeyDown={dialogKeyDown(close)}
      ref={dialog}
    >
      {open ? <ToolDatabaseBody onClose={close} /> : null}
    </dialog>
  );
}

function ToolDatabaseBody({ onClose }: { onClose: () => void }): JSX.Element {
  const database = useToolDatabase();
  const machines = usePrefsStore((state) => state.prefs.machines);
  const [picked, setPicked] = useState<Picked>(null);
  const [draft, setDraft] = useState<Tool | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [newKind, setNewKind] = useState<ToolKind>('endMill');
  const [formats, setFormats] = useState<NameFormats | null>(null);
  const [importing, setImporting] = useState(false);
  const [importMode, setImportMode] = useState<ToolImportMode>('import');
  const dragged = useRef<TreeItem | null>(null);
  const toolPick = useUiStore((ui) => ui.toolPick);
  const toolPickKinds = useUiStore((ui) => ui.toolPickKinds);
  const ids = { material: useId(), machine: useId(), kind: useId(), hint: useId() };

  const loadFormats = () =>
    api.tools
      .nameFormats()
      .then(setFormats)
      .catch(() => setFormats(null));
  useEffect(() => {
    void loadFormats();
  }, []);

  const pickedTool = useMemo(() => {
    if (draft !== null) {
      return draft;
    }
    return picked?.kind === 'tool'
      ? (database?.tools.find((t) => t.id === picked.id) ?? null)
      : null;
  }, [database, draft, picked]);

  const copyWhy = picked?.kind === 'tool' ? undefined : strings.why.pickToolInList;
  const removeWhy = picked === null ? strings.why.pickInList : undefined;
  const exportWhy = database === null ? strings.why.dbLoading : undefined;
  const selectWhy = firstWhy(
    [draft !== null, strings.why.saveDraft],
    [pickedTool === null, strings.why.pickToolInList],
  );

  const groupForNew = (): number => {
    if (picked?.kind === 'group') {
      return picked.id;
    }
    if (picked?.kind === 'tool') {
      return database?.tools.find((t) => t.id === picked.id)?.group ?? ROOT;
    }
    return ROOT;
  };

  const run = async (action: () => Promise<void>) => {
    setFailure(null);
    try {
      await action();
    } catch (error) {
      setFailure(message(error));
    }
    await reloadTools();
  };

  const context = {
    machine: database?.machine ?? machines.active,
    material: database?.activeMaterial ?? '',
  };

  const setMachine = async (name: string) => {
    if (await usePrefsStore.getState().update({ machines: { ...machines, active: name } })) {
      await reloadTools();
    }
  };

  const setMaterial = (name: string) =>
    run(async () => {
      await api.tools.activateMaterial(name);
    });

  const addGroup = () =>
    run(async () => {
      const parent = groupForNew();
      const reply = await api.tools.addGroup(text.newGroupName, parent === ROOT ? null : parent);
      setDraft(null);
      setPicked({ kind: 'group', id: reply.id });
    });

  const addDefaults = () =>
    run(async () => {
      const reply = await api.tools.addDefaults();
      setNotice(text.addedDefaults(reply.toolIds.length));
    });

  const copy = () =>
    run(async () => {
      if (picked?.kind !== 'tool') {
        return;
      }
      const reply = await api.tools.copy(picked.id);
      setPicked({ kind: 'tool', id: reply.id });
    });

  const remove = async () => {
    if (picked === null || database === null) {
      return;
    }
    const name =
      picked.kind === 'tool'
        ? database.tools.find((t) => t.id === picked.id)?.displayName
        : database.groups.find((g) => g.id === picked.id)?.name;
    const question =
      picked.kind === 'tool' ? text.confirmDelete(name ?? '') : text.confirmDeleteGroup(name ?? '');
    if (confirmationOn('deleteTool') && !(await getKernel().confirm(question))) {
      return;
    }
    await run(async () => {
      if (picked.kind === 'tool') {
        await api.tools.delete([picked.id]);
      } else {
        await api.tools.deleteGroup(picked.id);
      }
      setPicked(null);
    });
  };

  const place = (target: Placement | null) => {
    if (target === null) {
      return;
    }
    void run(async () => {
      await api.tools.place(target.item, target.group, target.before);
      setPicked(target.item);
    });
  };

  const importFile = async () => {
    const path = await getKernel().chooseOpenPath(text.importTitle, TOOL_DB_FILTER);
    if (path === null) {
      return;
    }
    await run(async () => {
      const reply = await api.tools.import(path, importMode, groupForNew());
      setNotice(text.imported(reply.toolIds.length, reply.updated.length));
      setImporting(false);
    });
  };

  const exportFile = async () => {
    const path = await getKernel().chooseSavePath(text.exportTitle, TOOL_DB_FILTER, 'tools.json');
    if (path === null) {
      return;
    }
    await run(async () => {
      const target =
        picked?.kind === 'tool'
          ? { toolIds: [picked.id] }
          : { group: picked?.kind === 'group' ? picked.id : ROOT };
      const reply = await api.tools.export(path, target);
      setNotice(text.exported(reply.path));
    });
  };

  const save = (tool: Tool, cutting: Parameters<typeof api.tools.setCuttingData>[1] | null) =>
    run(async () => {
      const reply = await api.tools.save(tool, tool.group ?? ROOT);
      if (cutting !== null) {
        await api.tools.setCuttingData(reply.id, cutting);
      }
      setDraft(null);
      setPicked({ kind: 'tool', id: reply.id });
      setNotice(text.saved);
    });

  const newTool = () => {
    setDraft(blankTool(groupForNew(), newKind, formats?.formats[newKind]));
    setPicked(null);
  };

  return (
    <div className="tool-database__body">
      <header className="tool-database__header">
        <h2 className="tool-database__title" id="tool-database-title">
          {text.title}
        </h2>
        <div className="tool-database__context">
          <label className="tool-database__label" htmlFor={ids.material}>
            {text.material}
          </label>
          <select
            className="tool-database__input"
            disabled={database === null}
            id={ids.material}
            onChange={(e) => void setMaterial(e.target.value)}
            value={context.material}
          >
            <option value="">{text.noMaterial}</option>
            {(database?.materials ?? []).map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          <button
            className="tool-database__button"
            onClick={() => useDialogStore.getState().show('materials')}
            type="button"
            data-tip={text.manageMaterials}
          >
            <ButtonFace icon="materialSetup" label={text.manageMaterials} />
          </button>
          <label className="tool-database__label" htmlFor={ids.machine}>
            {text.machine}
          </label>
          <select
            className="tool-database__input"
            id={ids.machine}
            onChange={(e) => void setMachine(e.target.value)}
            value={machines.active}
          >
            {machines.list.map((machine) => (
              <option key={machine.name} value={machine.name}>
                {machine.name}
              </option>
            ))}
          </select>
          <button
            className="tool-database__button"
            onClick={() => useDialogStore.getState().show('machineConfiguration')}
            type="button"
            data-tip={text.manageMachines}
          >
            <ButtonFace icon="machine" label={text.manageMachines} />
          </button>
        </div>
      </header>
      <div className="tool-database__panes">
        <nav aria-describedby={ids.hint} aria-label={text.tree} className="tool-database__tree">
          <div className="tool-database__toolbar">
            <label className="tool-database__visually-hidden" htmlFor={ids.kind}>
              {text.newToolOfType}
            </label>
            <select
              className="tool-database__input tool-database__kind"
              id={ids.kind}
              onChange={(e) => setNewKind(e.target.value as ToolKind)}
              value={newKind}
            >
              {EDITABLE_KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {text.kinds[kind]}
                </option>
              ))}
            </select>
            <button
              className="tool-database__button"
              onClick={newTool}
              type="button"
              data-tip={text.newTool}
            >
              <ButtonFace icon="formCreate" label={text.newTool} />
            </button>
            <button
              className="tool-database__button"
              onClick={() => void addGroup()}
              type="button"
              data-tip={text.newGroup}
            >
              <ButtonFace icon="folderAdd" label={text.newGroup} />
            </button>
            <button
              className="tool-database__button"
              disabled={copyWhy !== undefined}
              onClick={() => void copy()}
              type="button"
              data-tip={whyTip(text.copy, copyWhy)}
            >
              <ButtonFace icon="editCopy" label={text.copy} />
            </button>
            <button
              className="tool-database__button"
              disabled={removeWhy !== undefined}
              onClick={() => void remove()}
              type="button"
              data-tip={whyTip(text.remove, removeWhy)}
            >
              <ButtonFace icon="itemRemove" label={text.remove} />
            </button>
            <button
              aria-expanded={importing}
              className="tool-database__button"
              onClick={() => setImporting((on) => !on)}
              type="button"
              data-tip={text.importTools}
            >
              <ButtonFace icon="importModel" label={text.importTools} />
            </button>
            <button
              className="tool-database__button"
              disabled={exportWhy !== undefined}
              onClick={() => void exportFile()}
              type="button"
              data-tip={whyTip(text.exportTools, exportWhy)}
            >
              <ButtonFace icon="exportFile" label={text.exportTools} />
            </button>
          </div>
          {importing ? (
            <fieldset className="tool-database__import">
              <legend className="tool-database__legend">{text.importMode}</legend>
              {IMPORT_MODES.map((mode) => (
                <label className="tool-database__choice" key={mode}>
                  <input
                    checked={importMode === mode}
                    name="tool-import-mode"
                    onChange={() => setImportMode(mode)}
                    type="radio"
                  />
                  {text.importModes[mode]}
                </label>
              ))}
              <button
                className="tool-database__button tool-database__button--primary"
                onClick={() => void importFile()}
                type="button"
                data-tip={text.importDo}
              >
                <ButtonFace icon="importModel" label={text.importDo} />
              </button>
            </fieldset>
          ) : null}
          <p className="tool-database__hint" id={ids.hint}>
            {text.dragHint}
          </p>
          {database === null ? null : (
            <div
              className="tool-database__drop"
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => {
                event.preventDefault();
                const item = dragged.current;
                dragged.current = null;
                if (item !== null) {
                  place(placement(database, item, 'top'));
                }
              }}
            >
              <GroupList
                database={pickableDatabase(database, toolPick === null ? null : toolPickKinds)}
                dragged={dragged}
                onPick={(next) => {
                  setDraft(null);
                  setPicked(next);
                }}
                onPlace={place}
                parent={null}
                picked={picked}
              />
            </div>
          )}
        </nav>
        <section aria-label={text.geometry} className="tool-database__editor">
          {pickedTool !== null && database !== null ? (
            <>
              <p className="tool-database__crumbs">
                {text.breadcrumb(
                  draft !== null
                    ? (database.groups.find((g) => g.id === draft.group)?.path ?? [])
                    : breadcrumb(database, { kind: 'tool', id: pickedTool.id }),
                )}
              </p>
              <ToolEditor
                context={context}
                formats={formats}
                key={`${pickedTool.id}-${draft === null ? 'saved' : 'new'}-${database.dbRevision}-${context.material}-${context.machine}`}
                onFormatsChanged={() => {
                  void loadFormats();
                  void reloadTools();
                }}
                onSave={save}
                tool={pickedTool}
              />
            </>
          ) : picked?.kind === 'group' && database !== null ? (
            <GroupEditor
              key={groupEditorName(database, picked.id).key}
              name={groupEditorName(database, picked.id).name}
              onRename={(name) =>
                run(() => api.tools.renameGroup(picked.id, name).then(() => undefined))
              }
              path={breadcrumb(database, picked)}
            />
          ) : (
            <p className="tool-database__hint">{text.nothingSelected}</p>
          )}
        </section>
      </div>
      <footer className="tool-database__footer">
        {failure !== null ? (
          <p className="tool-database__error" role="alert">
            {failure}
          </p>
        ) : notice === null ? null : (
          <p aria-live="polite" className="tool-database__notice">
            {notice}
          </p>
        )}
        <button
          className="tool-database__button"
          onClick={() => void addDefaults()}
          type="button"
          data-tip={text.addDefaults}
        >
          <ButtonFace icon="formCreate" label={text.addDefaults} />
        </button>
        {toolPick === null ? null : (
          <button
            className="tool-database__button"
            disabled={selectWhy !== undefined}
            onClick={() => {
              if (pickedTool !== null) {
                toolPick(toolForToolpath(pickedTool));
                onClose();
              }
            }}
            type="button"
            data-tip={whyTip(text.select, selectWhy)}
          >
            <ButtonFace icon="formApply" label={text.select} />
          </button>
        )}
        <button
          className="tool-database__button"
          onClick={onClose}
          type="button"
          data-tip={text.close}
        >
          <ButtonFace icon="dismiss" label={text.close} />
        </button>
      </footer>
    </div>
  );
}

interface GroupListProps {
  database: ToolDatabaseList;
  parent: number | null;
  picked: Picked;
  onPick: (picked: Picked) => void;
  onPlace: (placement: Placement | null) => void;
  dragged: { current: TreeItem | null };
}

function GroupList(props: GroupListProps): JSX.Element {
  const { database, parent, picked, onPick, onPlace, dragged } = props;
  const groups = database.groups.filter((g) => g.parent === parent);
  const tools = database.tools.filter((t) => (t.group ?? ROOT) === (parent ?? ROOT));

  const handlers = (item: TreeItem) => ({
    draggable: true,
    onDragStart: (event: DragEvent<HTMLButtonElement>) => {
      dragged.current = item;
      event.dataTransfer.effectAllowed = 'move';
      event.dataTransfer.setData('text/plain', `${item.kind}:${item.id}`);
    },
    onDragOver: (event: DragEvent<HTMLButtonElement>) => {
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
    },
    onDrop: (event: DragEvent<HTMLButtonElement>) => {
      event.preventDefault();
      event.stopPropagation();
      const from = dragged.current;
      dragged.current = null;
      if (from !== null) {
        onPlace(placement(database, from, item));
      }
    },
    onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => {
      if (!event.altKey || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) {
        return;
      }
      event.preventDefault();
      onPlace(nudge(database, item, event.key === 'ArrowUp' ? 'up' : 'down'));
    },
  });

  return (
    <ul className="tool-database__list">
      {groups.map((group) => {
        const item: TreeItem = { kind: 'group', id: group.id };
        return (
          <li className="tool-database__group" key={`g${group.id}`}>
            <button
              {...handlers(item)}
              aria-pressed={picked?.kind === 'group' && picked.id === group.id}
              className="tool-database__item tool-database__item--group"
              onClick={() => onPick(item)}
              data-tip={text.breadcrumb(group.path)}
              type="button"
            >
              {group.name}
            </button>
            <GroupList {...props} parent={group.id} />
          </li>
        );
      })}
      {tools.map((tool) => {
        const item: TreeItem = { kind: 'tool', id: tool.id };
        return (
          <li key={`t${tool.id}`}>
            <button
              {...handlers(item)}
              aria-pressed={picked?.kind === 'tool' && picked.id === tool.id}
              className="tool-database__item"
              onClick={() => onPick(item)}
              data-tip={text.breadcrumb([
                ...breadcrumb(database, item),
                tool.displayName ?? tool.name,
              ])}
              type="button"
            >
              {tool.displayName ?? tool.name}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function GroupEditor({
  name,
  path,
  onRename,
}: {
  name: string;
  path: string[];
  onRename: (name: string) => void;
}): JSX.Element {
  const [value, setValue] = useState(name);
  const id = useId();
  return (
    <form
      className="tool-database__form"
      onSubmit={(event) => {
        event.preventDefault();
        if (value.trim() !== '') {
          onRename(value.trim());
        }
      }}
    >
      <p className="tool-database__crumbs">{text.breadcrumb(path)}</p>
      <label className="tool-database__label" htmlFor={id}>
        {text.groupName}
      </label>
      <input
        className="tool-database__input"
        id={id}
        onChange={(e) => setValue(e.target.value)}
        value={value}
      />
      <div className="tool-database__actions">
        <button
          className="tool-database__button tool-database__button--primary"
          type="submit"
          data-tip={text.renameGroup}
        >
          <ButtonFace icon="rename" label={text.renameGroup} />
        </button>
      </div>
    </form>
  );
}
