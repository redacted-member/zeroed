import { useId, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { reloadTools, useToolDatabase } from '../../lib/camData';
import { machineSetupStrings } from '../../lib/strings/machineSetup';
import { useDialogStore } from '../../state/dialogStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { nameProblem, nextAfterDelete, uniqueName } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { whyStrings } from '../../lib/strings/why';

import './MaterialManagement.scss';

const text = machineSetupStrings.materials;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Toolpaths > Materials: the materials tools keep cutting data for, and the active one. */
export function MaterialManagement(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'materials');
  const close = useDialogStore((state) => state.close);
  return (
    <ModalDialog
      className="material-management"
      closeLabel={text.close}
      onClose={close}
      open={open}
      title={text.title}
      titleId="material-management-title"
    >
      <MaterialBody />
    </ModalDialog>
  );
}

function MaterialBody(): JSX.Element {
  const database = useToolDatabase();
  const materials = database?.materials ?? [];
  const active = database?.activeMaterial ?? '';
  const activeWhy = active === '' ? whyStrings.why.noMaterial : undefined;
  // The typed name belongs to the material it was typed for.
  const [edited, setEdited] = useState<{ material: string; name: string } | null>(null);
  const name = edited !== null && edited.material === active ? edited.name : active;
  const setName = (value: string) => setEdited({ material: active, name: value });
  const [copyData, setCopyData] = useState(true);
  const [failure, setFailure] = useState<string | null>(null);
  const ids = { list: useId(), name: useId(), copy: useId(), hint: useId() };

  const run = async (action: () => Promise<void>) => {
    setFailure(null);
    try {
      await action();
    } catch (error) {
      setFailure(message(error));
    }
    await reloadTools();
  };

  const activate = (material: string) =>
    run(() => api.tools.activateMaterial(material).then(() => undefined));

  const add = () =>
    run(async () => {
      const created = uniqueName(materials, text.newName);
      await api.tools.addMaterial(created);
      await api.tools.activateMaterial(created);
    });

  const duplicate = () =>
    run(async () => {
      if (active === '') {
        return;
      }
      const created = uniqueName(materials, text.copyOf(active));
      await api.tools.duplicateMaterial(active, created, copyData);
      await api.tools.activateMaterial(created);
    });

  const remove = async () => {
    if (active === '' || !(await getKernel().confirm(text.confirmDelete(active)))) {
      return;
    }
    const next = nextAfterDelete(materials, active);
    await run(async () => {
      await api.tools.deleteMaterial(active);
      await api.tools.activateMaterial(next);
    });
  };

  const rename = () => {
    const problem = nameProblem(materials, active, name);
    if (problem !== null) {
      setFailure(problem);
      return;
    }
    if (name.trim() === active) {
      return;
    }
    void run(() => api.tools.renameMaterial(active, name.trim()).then(() => undefined));
  };

  return (
    <div className="material-management__layout">
      <section aria-labelledby={ids.list} className="material-management__list-pane">
        <h3 className="material-management__heading" id={ids.list}>
          {text.list}
        </h3>
        <p className="material-management__hint" id={ids.hint}>
          {text.activeHint}
        </p>
        {materials.length === 0 ? (
          <p className="material-management__hint">{text.none}</p>
        ) : (
          <ul aria-describedby={ids.hint} className="material-management__list">
            {materials.map((material) => (
              <li key={material}>
                <button
                  aria-pressed={material === active}
                  className="material-management__item"
                  onClick={() => void activate(material)}
                  type="button"
                >
                  {material}
                  {material === active ? (
                    <span className="material-management__badge">{text.active}</span>
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="material-management__buttons">
          <button
            className="material-management__button"
            onClick={() => void add()}
            type="button"
            data-tip={text.add}
          >
            <ButtonFace icon="formCreate" label={text.add} />
          </button>
          <button
            className="material-management__button"
            disabled={activeWhy !== undefined}
            onClick={() => void duplicate()}
            type="button"
            data-tip={whyTip(text.duplicate, activeWhy)}
          >
            <ButtonFace icon="editCopy" label={text.duplicate} />
          </button>
          <button
            className="material-management__button"
            disabled={activeWhy !== undefined}
            onClick={() => void remove()}
            type="button"
            data-tip={whyTip(text.remove, activeWhy)}
          >
            <ButtonFace icon="itemRemove" label={text.remove} />
          </button>
        </div>
        <div className="material-management__check">
          <input
            checked={copyData}
            id={ids.copy}
            onChange={(e) => setCopyData(e.target.checked)}
            type="checkbox"
          />
          <label htmlFor={ids.copy}>{text.copyData}</label>
        </div>
      </section>
      <form
        className="material-management__edit"
        onSubmit={(event) => {
          event.preventDefault();
          rename();
        }}
      >
        <label className="material-management__label" htmlFor={ids.name}>
          {text.name}
        </label>
        <input
          className="material-management__input"
          disabled={active === ''}
          id={ids.name}
          onChange={(e) => setName(e.target.value)}
          value={name}
        />
        <button
          className="material-management__button"
          disabled={activeWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.rename, activeWhy)}
        >
          <ButtonFace icon="rename" label={text.rename} />
        </button>
        {failure === null ? null : (
          <p className="material-management__error" role="alert">
            {failure}
          </p>
        )}
      </form>
    </div>
  );
}
