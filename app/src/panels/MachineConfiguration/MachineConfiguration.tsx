import { useEffect, useId, useRef, useState, type JSX, type MutableRefObject } from 'react';

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import type { Units } from '../../bridge/types';
import { reloadTools, usePosts } from '../../lib/camData';
import { machineSetupStrings } from '../../lib/strings/machineSetup';
import { unitSuffix } from '../../lib/units';
import { useDialogStore } from '../../state/dialogStore';
import { usePrefsStore } from '../../state/prefsStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import {
  ACCEL_UNITS,
  activate,
  activeMachine,
  addMachine,
  addPost,
  deleteMachine,
  draftFrom,
  draftPrefs,
  duplicateMachine,
  editActive,
  machineProblems,
  removePost,
  toolDbChanges,
  TIMING_FIELDS,
  TRAVEL_FIELDS,
  withAccelUnits,
  withUnits,
  type MachineDraft,
  type MachinesDraft,
} from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { whyStrings } from '../../lib/strings/why';

import './MachineConfiguration.scss';

const text = machineSetupStrings.machineConfig;
const UNITS: ReadonlyArray<{ value: Units; label: string }> = [
  { value: 'mm', label: text.mm },
  { value: 'inch', label: text.inch },
];

type RotaryValue = MachineDraft['rotaryLetter'] | MachineDraft['rotaryUnits'] | boolean;

/** The Rotary Axis group's rows: each a choice over one of the draft's fields. */
const ROTARY_CHOICES: ReadonlyArray<{
  label: 'rotaryLetter' | 'rotaryUnits' | 'rotaryDirection';
  choices: ReadonlyArray<{ value: RotaryValue; label: string }>;
  read: (machine: MachineDraft) => RotaryValue;
  write: (machine: MachineDraft, value: RotaryValue) => MachineDraft;
}> = [
  {
    label: 'rotaryLetter',
    choices: [
      { value: 'A', label: 'A' },
      { value: 'B', label: 'B' },
    ],
    read: (machine) => machine.rotaryLetter,
    write: (machine, value) => ({ ...machine, rotaryLetter: value === 'B' ? 'B' : 'A' }),
  },
  {
    label: 'rotaryUnits',
    choices: [
      { value: 'degrees', label: text.rotaryDegrees },
      { value: 'linear', label: text.rotaryLinear },
    ],
    read: (machine) => machine.rotaryUnits,
    write: (machine, value) => ({
      ...machine,
      rotaryUnits: value === 'linear' ? 'linear' : 'degrees',
    }),
  },
  {
    label: 'rotaryDirection',
    choices: [
      { value: false, label: text.rotaryNormal },
      { value: true, label: text.rotaryReversed },
    ],
    read: (machine) => machine.rotaryReversed,
    write: (machine, value) => ({ ...machine, rotaryReversed: value === true }),
  },
];

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Machine > Machine Configuration: the machines and their post processors. */
export function MachineConfiguration(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'machineConfiguration');
  const close = useDialogStore((state) => state.close);
  const submit = useRef<(() => Promise<boolean>) | null>(null);
  const [busy, setBusy] = useState(false);
  const busyWhy = busy ? whyStrings.why.busy : undefined;

  const run = async (then: boolean) => {
    const apply = submit.current;
    if (apply === null || busy) {
      return;
    }
    setBusy(true);
    const ok = await apply();
    setBusy(false);
    if (ok && then) {
      close();
    }
  };

  return (
    <ModalDialog
      className="machine-config"
      closeLabel={text.cancel}
      footer={
        <>
          <button
            className="modal-dialog__button"
            disabled={busyWhy !== undefined}
            onClick={() => void run(false)}
            type="button"
            data-tip={whyTip(text.apply, busyWhy)}
          >
            <ButtonFace icon="formApply" label={text.apply} />
          </button>
          <button
            className="modal-dialog__button"
            disabled={busyWhy !== undefined}
            onClick={() => void run(true)}
            type="button"
            data-tip={whyTip(text.ok, busyWhy)}
          >
            <ButtonFace icon="formApply" label={text.ok} />
          </button>
        </>
      }
      onClose={close}
      open={open}
      title={text.title}
      titleId="machine-config-title"
    >
      <MachineBody submitRef={submit} />
    </ModalDialog>
  );
}

function MachineBody({
  submitRef,
}: {
  submitRef: MutableRefObject<(() => Promise<boolean>) | null>;
}): JSX.Element {
  const prefs = usePrefsStore((state) => state.prefs.machines);
  const posts = usePosts();
  const [initial, setInitial] = useState<string[]>(() => prefs.list.map((m) => m.name));
  // Machine > Add Machine opens the dialog on a new machine; the hand-off is used once.
  const [draft, setDraft] = useState<MachinesDraft>(() =>
    useDialogStore.getState().addMachine
      ? addMachine(draftFrom(prefs), usePrefsStore.getState().prefs.general.defaultUnits)
      : draftFrom(prefs),
  );
  useEffect(() => {
    useDialogStore.getState().setAddMachine(false);
  }, []);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [chosenPost, setChosenPost] = useState('');
  const ids = { list: useId(), name: useId(), post: useId(), units: useId() };
  const machine = activeMachine(draft);
  const machineWhy = machine === null ? whyStrings.why.noMachine : undefined;
  const addPostWhy = chosenPost === '' ? whyStrings.why.choosePost : undefined;
  const problems = machine === null ? {} : machineProblems(draft, machine);
  const postName = (id: string) => posts?.find((p) => p.id === id)?.name ?? id;

  const apply = async (): Promise<boolean> => {
    setStatus(null);
    setFailure(null);
    const next = draftPrefs(draft);
    if (next === null) {
      return false;
    }
    try {
      const known = (await api.tools.list()).machines;
      const changes = toolDbChanges(initial, draft, known);
      for (const name of changes.remove) {
        await api.tools.deleteMachine(name);
      }
      for (const { from, to } of changes.rename) {
        await api.tools.renameMachine(from, to);
      }
      for (const name of changes.add) {
        await api.tools.addMachine(name);
      }
      await reloadTools();
    } catch (error) {
      setFailure(message(error));
      return false;
    }
    if (!(await usePrefsStore.getState().update({ machines: next }))) {
      setFailure(usePrefsStore.getState().error ?? text.title);
      return false;
    }
    // Tools resolve their cutting data for the active machine.
    await reloadTools();
    const saved = usePrefsStore.getState().prefs.machines;
    setInitial(saved.list.map((m) => m.name));
    setDraft(draftFrom(saved));
    setStatus(text.saved);
    return true;
  };
  useEffect(() => {
    submitRef.current = apply;
    return () => {
      submitRef.current = null;
    };
  });

  const onDelete = async () => {
    if (machine === null) {
      return;
    }
    const name = machine.original ?? machine.name;
    if (machine.original !== null && !(await getKernel().confirm(text.removeConfirm(name)))) {
      return;
    }
    setDraft((current) => deleteMachine(current));
  };

  return (
    <div className="machine-config__layout">
      <section aria-labelledby={ids.list} className="machine-config__machines">
        <h3 className="machine-config__heading" id={ids.list}>
          {text.machines}
        </h3>
        {draft.machines.length === 0 ? <p className="machine-config__hint">{text.none}</p> : null}
        <ul aria-labelledby={ids.list} className="machine-config__list" role="listbox">
          {draft.machines.map((m) => (
            <li
              aria-selected={m.key === draft.active}
              className="machine-config__item"
              key={m.key}
              onClick={() => setDraft((current) => activate(current, m.key))}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  setDraft((current) => activate(current, m.key));
                }
              }}
              role="option"
              tabIndex={0}
            >
              {m.name.trim() === '' ? '...' : m.name}
            </li>
          ))}
        </ul>
        <div className="machine-config__buttons">
          <button
            className="modal-dialog__button"
            onClick={() =>
              setDraft((current) =>
                addMachine(current, usePrefsStore.getState().prefs.general.defaultUnits),
              )
            }
            type="button"
            data-tip={text.add}
          >
            <ButtonFace icon="formCreate" label={text.add} />
          </button>
          <button
            className="modal-dialog__button"
            disabled={machineWhy !== undefined}
            onClick={() => setDraft((current) => duplicateMachine(current))}
            type="button"
            data-tip={whyTip(text.duplicate, machineWhy)}
          >
            <ButtonFace icon="editCopy" label={text.duplicate} />
          </button>
          <button
            className="modal-dialog__button"
            disabled={machineWhy !== undefined}
            onClick={() => void onDelete()}
            type="button"
            data-tip={whyTip(text.remove, machineWhy)}
          >
            <ButtonFace icon="itemRemove" label={text.remove} />
          </button>
        </div>
      </section>
      {machine === null ? null : (
        <div className="machine-config__details">
          <fieldset className="machine-config__group">
            <legend className="machine-config__legend">{text.details}</legend>
            <div className="machine-config__row">
              <label className="machine-config__label" htmlFor={ids.name}>
                {text.name}
              </label>
              <input
                aria-invalid={problems.name !== undefined}
                className="machine-config__input"
                id={ids.name}
                onChange={(event) => {
                  const name = event.target.value;
                  setDraft((current) => editActive(current, (m) => ({ ...m, name })));
                }}
                type="text"
                value={machine.name}
              />
              {problems.name !== undefined ? (
                <p className="machine-config__problem" role="alert">
                  {problems.name}
                </p>
              ) : null}
            </div>
            <div className="machine-config__row" role="radiogroup" aria-labelledby={ids.units}>
              <span className="machine-config__label" id={ids.units}>
                {text.units}
              </span>
              <span className="machine-config__choices">
                {UNITS.map((choice) => (
                  <label className="machine-config__choice" key={choice.value}>
                    <input
                      checked={machine.units === choice.value}
                      name="machine-config-units"
                      onChange={() =>
                        setDraft((current) =>
                          editActive(current, (m) => withUnits(m, choice.value)),
                        )
                      }
                      type="radio"
                    />
                    {choice.label}
                  </label>
                ))}
              </span>
            </div>
          </fieldset>
          <fieldset className="machine-config__group">
            <legend className="machine-config__legend">{text.travel}</legend>
            {TRAVEL_FIELDS.map((field) => (
              <div className="machine-config__row" key={field}>
                <label className="machine-config__label" htmlFor={`${ids.name}-${field}`}>
                  {text[field]}
                </label>
                <span className="machine-config__control">
                  <input
                    aria-invalid={problems[field] !== undefined}
                    className="machine-config__number"
                    id={`${ids.name}-${field}`}
                    inputMode="decimal"
                    onChange={(event) => {
                      const value = event.target.value;
                      setDraft((current) => editActive(current, (m) => ({ ...m, [field]: value })));
                    }}
                    type="text"
                    value={machine[field]}
                  />
                  <span className="machine-config__unit">{unitSuffix(machine.units)}</span>
                </span>
                {problems[field] !== undefined ? (
                  <p className="machine-config__problem" role="alert">
                    {problems[field]}
                  </p>
                ) : null}
              </div>
            ))}
          </fieldset>
          <fieldset className="machine-config__group">
            <legend className="machine-config__legend">{text.timing}</legend>
            <div className="machine-config__row">
              <label className="machine-config__label" htmlFor={`${ids.name}-accel-units`}>
                {text.accelUnits}
              </label>
              <span className="machine-config__control">
                <select
                  className="machine-config__select"
                  id={`${ids.name}-accel-units`}
                  onChange={(event) => {
                    const unit = ACCEL_UNITS.find((u) => u === event.target.value);
                    if (unit !== undefined) {
                      setDraft((current) => editActive(current, (m) => withAccelUnits(m, unit)));
                    }
                  }}
                  value={machine.accelUnits}
                >
                  {ACCEL_UNITS.map((unit) => (
                    <option key={unit} value={unit}>
                      {text.accelUnitNames[unit]}
                    </option>
                  ))}
                </select>
              </span>
            </div>
            {TIMING_FIELDS.map((field) => (
              <div className="machine-config__row" key={field}>
                <label className="machine-config__label" htmlFor={`${ids.name}-${field}`}>
                  {text[field]}
                </label>
                <span className="machine-config__control">
                  <input
                    aria-invalid={problems[field] !== undefined}
                    className="machine-config__number"
                    id={`${ids.name}-${field}`}
                    inputMode="decimal"
                    onChange={(event) => {
                      const value = event.target.value;
                      setDraft((current) => editActive(current, (m) => ({ ...m, [field]: value })));
                    }}
                    type="text"
                    value={machine[field]}
                  />
                  <span className="machine-config__unit">
                    {field === 'rapidRate'
                      ? text.perMinute(unitSuffix(machine.units))
                      : field === 'rotaryAccel'
                        ? text.degreesPerSecondSquared
                        : text.accelUnitNames[machine.accelUnits]}
                  </span>
                </span>
                {problems[field] !== undefined ? (
                  <p className="machine-config__problem" role="alert">
                    {problems[field]}
                  </p>
                ) : null}
              </div>
            ))}
            <p className="machine-config__hint">{text.timingHint}</p>
          </fieldset>
          <fieldset className="machine-config__group">
            <legend className="machine-config__legend">{text.rotaryAxis}</legend>
            {ROTARY_CHOICES.map((group) => (
              <div
                aria-label={text[group.label]}
                className="machine-config__row"
                key={group.label}
                role="radiogroup"
              >
                <span className="machine-config__label">{text[group.label]}</span>
                <span className="machine-config__choices">
                  {group.choices.map((choice) => (
                    <label className="machine-config__choice" key={choice.label}>
                      <input
                        checked={group.read(machine) === choice.value}
                        name={`machine-config-${group.label}`}
                        onChange={() =>
                          setDraft((current) =>
                            editActive(current, (m) => group.write(m, choice.value)),
                          )
                        }
                        type="radio"
                      />
                      {choice.label}
                    </label>
                  ))}
                </span>
              </div>
            ))}
            <p className="machine-config__hint">{text.rotaryHint}</p>
          </fieldset>
          <fieldset className="machine-config__group">
            <legend className="machine-config__legend">{text.posts}</legend>
            {machine.posts.length === 0 ? (
              <p className="machine-config__hint">{text.noPosts}</p>
            ) : (
              <ul className="machine-config__posts">
                {machine.posts.map((post) => (
                  <li className="machine-config__post" key={post}>
                    <label className="machine-config__choice">
                      <input
                        checked={machine.defaultPost === post}
                        name="machine-config-default-post"
                        onChange={() =>
                          setDraft((current) =>
                            editActive(current, (m) => ({ ...m, defaultPost: post })),
                          )
                        }
                        data-tip={text.defaultPost}
                        type="radio"
                      />
                      {postName(post)}
                    </label>
                    <button
                      aria-label={text.removePostNamed(postName(post))}
                      className="modal-dialog__button"
                      onClick={() =>
                        setDraft((current) => editActive(current, (m) => removePost(m, post)))
                      }
                      type="button"
                      data-tip={text.removePost}
                    >
                      <ButtonFace icon="itemRemove" label={text.removePost} />
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {machine.posts.length > 0 ? (
              <p className="machine-config__hint">{text.defaultPost}</p>
            ) : null}
            <div className="machine-config__add-post">
              <label className="machine-config__label" htmlFor={ids.post}>
                {text.addPost}
              </label>
              <select
                className="machine-config__select"
                id={ids.post}
                onChange={(event) => setChosenPost(event.target.value)}
                value={chosenPost}
              >
                <option value="">{text.choosePost}</option>
                {(posts ?? [])
                  .filter((p) => !machine.posts.includes(p.id))
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </select>
              <button
                className="modal-dialog__button"
                disabled={addPostWhy !== undefined}
                onClick={() => {
                  const post = chosenPost;
                  setDraft((current) => editActive(current, (m) => addPost(m, post)));
                  setChosenPost('');
                }}
                type="button"
                data-tip={whyTip(text.addPostButton, addPostWhy)}
              >
                <ButtonFace icon="formCreate" label={text.addPostButton} />
              </button>
            </div>
          </fieldset>
        </div>
      )}
      {failure !== null ? (
        <p className="machine-config__problem machine-config__status" role="alert">
          {failure}
        </p>
      ) : null}
      {status !== null ? (
        <p className="machine-config__hint machine-config__status" role="status">
          {status}
        </p>
      ) : null}
    </div>
  );
}
