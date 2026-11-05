import { useState, type JSX } from 'react';

import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { usePosts } from '../../lib/camData';
import { ROTARY_JOBS_TOPIC } from '../../lib/helpTopics';
import { strings } from '../../lib/strings';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { machineWrapsBlank } from './model';

import './RotaryGuide.scss';

const text = strings.rightPanel.rotaryGuide;

/**
 * On a rotary job, at the top of the Toolpaths panel: how the job gets from
 * here onto the blank. The steps are open until the job has a toolpath; once
 * opened or closed by hand they stay as the user left them.
 */
export function RotaryGuide(): JSX.Element | null {
  const axis = useDocumentStore((s) => (s.job.kind === 'rotary' ? s.job.rotary.axis : null));
  const empty = useToolpathStore((s) => s.toolpaths.length === 0);
  const machines = usePrefsStore((s) => s.prefs.machines);
  const posts = usePosts();
  const [chosen, setChosen] = useState<boolean | null>(null);
  if (axis === null) {
    return null;
  }
  const open = chosen ?? empty;
  return (
    <div className="rotary-guide">
      <p className="rotary-guide__note">{text.note}</p>
      {machineWrapsBlank(posts, machines, axis) ? null : (
        <div className="rotary-guide__notice" role="status">
          <p>{text.noPost(axis.toUpperCase())}</p>
          <button
            className="rotary-guide__button"
            data-tip={text.machineTip}
            onClick={() => useDialogStore.getState().show('machineConfiguration')}
            type="button"
          >
            <ButtonFace icon="machine" label={text.machine} />
          </button>
        </div>
      )}
      <details
        className="rotary-guide__details"
        onToggle={(event) => {
          if (event.currentTarget.open !== open) {
            setChosen(event.currentTarget.open);
          }
        }}
        open={open}
      >
        <summary className="rotary-guide__summary">{text.title}</summary>
        <ol className="rotary-guide__steps">
          {text.steps.map((step) => (
            <li className="rotary-guide__step" key={step.name}>
              <strong>{step.name}</strong> {step.text}
            </li>
          ))}
        </ol>
        <button
          className="rotary-guide__button"
          data-tip={text.helpTip}
          onClick={() => useDialogStore.getState().showHelp(ROTARY_JOBS_TOPIC)}
          type="button"
        >
          <ButtonFace icon="help" label={text.help} />
        </button>
      </details>
    </div>
  );
}
