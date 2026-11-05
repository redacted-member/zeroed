import { useId, useState, type JSX } from 'react';

import { rotaryWiring } from '../../bridge/prefs';
import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { usePosts } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { CheckField } from '../CheckField/CheckField';
import { canOutputDirect, firstPost, postsForMachine } from '../SaveToolpathsForm/model';
import { immediateFileName, immediateProblem, immediateSaveArgs, immediateToolpath } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ImmediateOutput.scss';

const text = strings.toolpaths.immediate;

interface ImmediateOutputProps {
  /** The toolpath the form calculated last, or null before the first Calculate. */
  toolpathId: number | null;
}

/** Saves the toolpath a form just calculated without closing the form. */
export function ImmediateOutput({ toolpathId }: ImmediateOutputProps): JSX.Element {
  const toolpaths = useToolpathStore((store) => store.toolpaths);
  const posts = usePosts();
  const machines = usePrefsStore((store) => store.prefs.machines);
  const doubleSided = useDocumentStore((s) => s.job.kind === 'doubleSided');
  const activeSide = useDocumentStore((s) => s.activeSide);
  const [machine, setMachine] = useState(machines.active);
  const [postChoice, setPostChoice] = useState<string | null>(null);
  const [addSide, setAddSide] = useState(false);
  const [directChoice, setDirectChoice] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const machineId = useId();
  const postId = useId();

  const offered = posts === null ? [] : postsForMachine(posts, machines, machine);
  const chosen = postChoice ?? (posts === null ? '' : firstPost(posts, machines, machine));
  const post = offered.find((p) => p.id === chosen) ?? offered[0] ?? null;
  const toolpath = immediateToolpath(toolpaths, toolpathId);
  const problem = immediateProblem(toolpath, post);
  const saveWhy = problem ?? undefined;
  const driver = canOutputDirect(post, getKernel());
  const direct = driver !== null && directChoice;

  const save = async () => {
    if (toolpath === null || post === null || problem !== null) {
      return;
    }
    const side =
      doubleSided && addSide
        ? activeSide === 'top'
          ? strings.toolpaths.save.sideTop
          : strings.toolpaths.save.sideBottom
        : null;
    const client = getKernel();
    const name = immediateFileName(toolpath, post, side);
    try {
      const path =
        direct && client.directOutputPath !== undefined
          ? await client.directOutputPath(name)
          : await client.chooseSavePath(
              strings.toolpaths.save.dialogTitle,
              [{ name: post.name, extensions: [post.fileExtension] }],
              name,
            );
      if (path === null) {
        return;
      }
      const reply = await api.cam.save(
        immediateSaveArgs(
          toolpath,
          post,
          path,
          rotaryWiring(machines.list.find((m) => m.name === machine)),
        ),
      );
      if (direct && driver !== null && client.sendToDriver !== undefined) {
        await client.sendToDriver(reply.paths, driver);
        setMessage(strings.toolpaths.save.sent(reply.paths.length, driver));
      } else {
        setMessage(text.saved(toolpath.name));
      }
      useDocumentStore.setState({ lastError: null });
    } catch (error) {
      useDocumentStore.setState({
        lastError: error instanceof Error ? error.message : String(error),
      });
    }
  };

  return (
    <fieldset className="immediate-output">
      <legend className="immediate-output__legend">{text.legend}</legend>
      <label className="immediate-output__field" htmlFor={machineId}>
        {text.machine}
        <select
          className="immediate-output__input"
          disabled={machines.list.length === 0}
          id={machineId}
          onChange={(event) => {
            setMachine(event.target.value);
            setPostChoice(null);
          }}
          value={machine}
        >
          {machines.list.length === 0 ? (
            <option value="">{strings.toolpaths.save.noMachine}</option>
          ) : null}
          {machines.list.map((m) => (
            <option key={m.name} value={m.name}>
              {m.name}
            </option>
          ))}
        </select>
      </label>
      <label className="immediate-output__field" htmlFor={postId}>
        {text.post}
        <select
          className="immediate-output__input"
          disabled={posts === null}
          id={postId}
          onChange={(event) => setPostChoice(event.target.value)}
          value={post?.id ?? ''}
        >
          {posts === null ? <option value="">{strings.toolpaths.save.loadingPosts}</option> : null}
          {offered.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <CheckField
        checked={addSide}
        disabled={!doubleSided}
        label={text.addSide}
        onChange={setAddSide}
      />
      <CheckField
        checked={direct}
        disabled={driver === null}
        label={text.direct}
        onChange={setDirectChoice}
      />
      <button
        className="immediate-output__save"
        disabled={saveWhy !== undefined}
        onClick={() => void save()}
        data-tip={whyTip(direct ? strings.toolpaths.save.outputAction : text.action, saveWhy)}
        type="button"
      >
        <ButtonFace
          icon="saveToolpaths"
          label={direct ? strings.toolpaths.save.outputAction : text.action}
        />
      </button>
      {message === null ? null : (
        <p aria-live="polite" className="immediate-output__hint">
          {message}
        </p>
      )}
    </fieldset>
  );
}
