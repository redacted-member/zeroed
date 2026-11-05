import { useEffect, useId, useState, type JSX } from 'react';

import { rotaryWiring } from '../../bridge/prefs';
import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { usePosts } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { BEFORE_YOU_CUT_TOPIC } from '../../lib/helpTopics';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { unitSuffix } from '../../lib/units';
import {
  canOutputDirect,
  firstPost,
  initialOutput,
  postsForMachine,
  savedToolpaths,
  suggestedName,
  toolChangeProblem,
  capabilityProblem,
  rotaryNote,
  rotaryWarning,
  trialCut,
  trialDefault,
  trialName,
  type SaveOutput,
  type TrialChoice,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './SaveToolpathsForm.scss';

const text = strings.toolpaths.save;

const OUTPUTS: ReadonlyArray<Choice<SaveOutput>> = [
  { value: 'selected', label: text.selected },
  { value: 'oneFile', label: text.oneFile },
  { value: 'multipleFiles', label: text.multipleFiles },
];

const TRIALS: ReadonlyArray<Choice<TrialChoice>> = [
  { value: 'none', label: text.trialNone },
  { value: 'air', label: text.trialAir },
  { value: 'scrap', label: text.trialScrap },
];

/** The post list entry that opens Machine Configuration. */
const ADD_POSTS = '__add';

export function SaveToolpathsForm(): JSX.Element {
  const toolpaths = useToolpathStore((store) => store.toolpaths);
  const selectedId = useToolpathStore((store) => store.selectedId);
  const posts = usePosts();
  const closeForm = useUiStore((store) => store.closeForm);
  const machines = usePrefsStore((store) => store.prefs.machines);
  // Opened from one toolpath's menu, the form saves that toolpath; otherwise
  // it starts on the choice it last saved with.
  const [output, setOutput] = useState<SaveOutput>(() =>
    initialOutput(
      useToolpathStore.getState().saveChoice,
      usePrefsStore.getState().prefs.toolpaths.saveOutput,
    ),
  );
  useEffect(() => {
    const store = useToolpathStore.getState();
    const choice = store.saveChoice;
    if (choice !== null && choice.length === 1 && choice[0] !== undefined) {
      store.select(choice[0]);
    }
    store.setSaveChoice(null);
  }, []);
  const [group, setGroup] = useState(false);
  const [machine, setMachine] = useState(machines.active);
  const [postChoice, setPostChoice] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const machineFieldId = useId();
  const postFieldId = useId();
  // Output Tiled Toolpaths follows whether Toolpath Tiling is on.
  const tilingOn = useToolpathStore((store) => store.tiling?.enabled === true);
  const [tiledChoice, setTiledChoice] = useState<boolean | null>(null);
  const outputTiled = tilingOn && (tiledChoice ?? true);

  const offered = posts === null ? [] : postsForMachine(posts, machines, machine);
  const jobUnits = useDocumentStore((s) => s.job.units);
  const saved = savedToolpaths(output, toolpaths, selectedId);
  const laser = saved.some((t) => t.kind === 'laser');
  const rotaryAxis = useDocumentStore((s) => (s.job.kind === 'rotary' ? s.job.rotary.axis : null));
  const postId =
    postChoice ??
    (posts === null ? '' : firstPost(posts, machines, machine, jobUnits, laser, rotaryAxis));
  const post = offered.find((p) => p.id === postId) ?? offered[0] ?? null;
  const doubleSided = useDocumentStore((s) => s.job.kind === 'doubleSided');
  const activeSide = useDocumentStore((s) => s.activeSide);
  const [addSide, setAddSide] = useState(false);
  const [directChoice, setDirectChoice] = useState(false);
  const directDriver = canOutputDirect(post, getKernel());
  const direct = directDriver !== null && directChoice;
  const problem = toolChangeProblem(output, saved, post) ?? capabilityProblem(saved, post);
  const warning = rotaryWarning(post, rotaryAxis);
  const wiring = rotaryWiring(machines.list.find((m) => m.name === machine));
  const wrapNote = rotaryNote(post, rotaryAxis, wiring);
  const m6AppliesLength = usePrefsStore((s) => s.prefs.toolpaths.m6AppliesLength);
  const lengthChoice = post?.lengthChoice === true;
  const [trialChoice, setTrialChoice] = useState<TrialChoice>('none');
  const [trialText, setTrialText] = useState('');
  const trial = trialCut(trialChoice, trialText, jobUnits);
  const pickTrial = (choice: TrialChoice) => {
    setTrialChoice(choice);
    setTrialText(trialDefault(choice, jobUnits));
  };
  const store = () => useToolpathStore.getState();

  const pickPost = (value: string) => {
    if (value === ADD_POSTS) {
      useDialogStore.getState().show('machineConfiguration');
      return;
    }
    setPostChoice(value);
  };

  const save = async () => {
    if (saved.length === 0 || post === null || problem !== null || trial.error !== null) {
      setMessage(output === 'selected' ? text.noSelection : text.none);
      return;
    }
    const client = getKernel();
    const name = trialName(
      suggestedName(
        output,
        saved,
        post.fileExtension,
        doubleSided && addSide ? (activeSide === 'top' ? text.sideTop : text.sideBottom) : null,
      ),
      trialChoice,
    );
    try {
      const path =
        direct && client.directOutputPath !== undefined
          ? await client.directOutputPath(name)
          : await client.chooseSavePath(
              text.dialogTitle,
              [{ name: post.name, extensions: [post.fileExtension] }],
              name,
            );
      if (path === null) {
        return;
      }
      const reply = await api.cam.save({
        toolpathIds: saved.map((t) => t.id),
        post: post.id,
        path,
        output,
        groupWherePossible: output === 'multipleFiles' && group,
        outputTiled,
        m6AppliesLength: !lengthChoice || m6AppliesLength,
        rotaryWiring: wiring,
        trialCut: trial.trial,
      });
      if (direct && directDriver !== null && client.sendToDriver !== undefined) {
        await client.sendToDriver(reply.paths, directDriver);
        setMessage(text.sent(reply.paths.length, directDriver));
      } else {
        setMessage(outputTiled ? text.tiled(reply.paths.length) : text.saved(reply.paths.length));
      }
      useDocumentStore.setState({ lastError: null });
      if (usePrefsStore.getState().prefs.toolpaths.saveOutput !== output) {
        void usePrefsStore.getState().update({ toolpaths: { saveOutput: output } });
      }
    } catch (error) {
      useDocumentStore.setState({
        lastError: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const saveWhy = firstWhy(
    [saved.length === 0, text.none],
    [post === null, strings.why.choosePost],
    [problem !== null, problem ?? ''],
    [trial.error !== null, trial.error ?? ''],
  );
  return (
    <div className="save-toolpaths">
      <p className="save-toolpaths__hint">{text.hint}</p>
      <div className="save-toolpaths__group">
        <ChoiceField label={text.output} onChange={setOutput} options={OUTPUTS} value={output} />
        <CheckField
          checked={group}
          disabled={output !== 'multipleFiles'}
          label={text.groupWherePossible}
          onChange={setGroup}
        />
        <CheckField
          checked={outputTiled}
          disabled={!tilingOn}
          label={text.outputTiled}
          onChange={setTiledChoice}
        />
        {tilingOn ? null : <p className="save-toolpaths__hint">{text.tilingOff}</p>}
      </div>
      <fieldset className="save-toolpaths__group">
        <legend className="save-toolpaths__legend">{text.which}</legend>
        {saved.length === 0 ? (
          <p className="save-toolpaths__hint">
            {output === 'selected' ? text.noSelection : text.none}
          </p>
        ) : (
          <ul className="save-toolpaths__list">
            {saved.map((toolpath) => (
              <li
                className={
                  toolpath.id === selectedId
                    ? 'save-toolpaths__row save-toolpaths__row--selected'
                    : 'save-toolpaths__row'
                }
                key={toolpath.id}
              >
                <input
                  aria-label={text.visible(toolpath.name)}
                  checked={toolpath.visible}
                  onChange={(event) => void store().setVisible(toolpath.id, event.target.checked)}
                  type="checkbox"
                />
                <button
                  aria-pressed={toolpath.id === selectedId}
                  className="save-toolpaths__name-button"
                  onClick={() => store().select(toolpath.id)}
                  type="button"
                >
                  {toolpath.name}
                  <span className="save-toolpaths__tool">
                    {text.toolNumber(toolpath.toolNumber, toolpath.toolName)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </fieldset>
      <fieldset className="save-toolpaths__group">
        <legend className="save-toolpaths__legend">{text.post}</legend>
        <label className="save-toolpaths__name" htmlFor={machineFieldId}>
          {text.machine}
          <select
            className="save-toolpaths__input"
            disabled={machines.list.length === 0}
            id={machineFieldId}
            onChange={(event) => {
              setMachine(event.target.value);
              setPostChoice(null);
            }}
            value={machine}
          >
            {machines.list.length === 0 ? <option value="">{text.noMachine}</option> : null}
            {machines.list.map((m) => (
              <option key={m.name} value={m.name}>
                {m.name}
              </option>
            ))}
          </select>
        </label>
        <label className="save-toolpaths__name" htmlFor={postFieldId}>
          {text.post}
          <select
            className="save-toolpaths__input"
            disabled={posts === null}
            id={postFieldId}
            onChange={(event) => pickPost(event.target.value)}
            value={post?.id ?? ''}
          >
            {posts === null ? <option value="">{text.loadingPosts}</option> : null}
            {offered.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
            {posts === null ? null : <option value={ADD_POSTS}>{text.addPosts}</option>}
          </select>
        </label>
        <CheckField
          checked={m6AppliesLength}
          disabled={!lengthChoice}
          label={text.m6Length}
          onChange={(on) =>
            void usePrefsStore.getState().update({ toolpaths: { m6AppliesLength: on } })
          }
          tip={lengthChoice ? text.m6LengthTip : text.m6LengthOff}
        />
        <ChoiceField label={text.trial} onChange={pickTrial} options={TRIALS} value={trialChoice} />
        {trialChoice === 'none' ? null : (
          <>
            <NumberField
              error={trial.error ?? undefined}
              label={trialChoice === 'air' ? text.airHeight : text.scrapDepth}
              onChange={setTrialText}
              unit={unitSuffix(jobUnits)}
              value={trialText}
            />
            <p className="save-toolpaths__hint">
              {trialChoice === 'air' ? text.airHint : text.scrapHint}
            </p>
          </>
        )}
        <CheckField
          checked={direct}
          disabled={directDriver === null}
          label={text.direct}
          onChange={setDirectChoice}
        />
        <button
          className="save-toolpaths__machines"
          onClick={() => useDialogStore.getState().show('machineConfiguration')}
          type="button"
          data-tip={strings.menuBar.items.machineConfiguration}
        >
          <ButtonFace icon="machine" label={strings.menuBar.items.machineConfiguration} />
        </button>
      </fieldset>
      <CheckField
        checked={addSide}
        disabled={!doubleSided}
        label={text.addSide}
        onChange={setAddSide}
      />
      <button
        className="save-toolpaths__save"
        disabled={saveWhy !== undefined}
        onClick={() => void save()}
        type="button"
        data-tip={whyTip(direct ? text.outputAction : text.action, saveWhy)}
      >
        <ButtonFace icon="saveToolpaths" label={direct ? text.outputAction : text.action} />
      </button>
      <button
        className="save-toolpaths__machines"
        onClick={() => useDialogStore.getState().showHelp(BEFORE_YOU_CUT_TOPIC)}
        type="button"
        data-tip={text.beforeYouCutTip}
      >
        <ButtonFace icon="help" label={text.beforeYouCut} />
      </button>
      {problem === null ? null : (
        <p className="save-toolpaths__notice" role="alert">
          {problem}
        </p>
      )}
      {warning === null ? null : (
        <p className="save-toolpaths__notice" role="status">
          {warning}
        </p>
      )}
      {wrapNote === null ? null : <p className="save-toolpaths__hint">{wrapNote}</p>}
      {message === null ? null : (
        <p aria-live="polite" className="save-toolpaths__hint">
          {message}
        </p>
      )}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
