import { useState, type JSX } from 'react';

import { strings } from '../../lib/formStrings';

import './AssemblyGuide.scss';

const text = strings.assembly.guide;

/**
 * The steps from a drawing to an assembled, exploded view, at the top of the
 * Assembly View. It is open by itself while the job has no pieces and not
 * closed again by the first piece; once opened or closed by hand it stays as
 * the user left it.
 */
export function AssemblyGuide({ empty }: { empty: boolean }): JSX.Element {
  const [chosen, setChosen] = useState<boolean | null>(null);
  const open = chosen ?? empty;
  return (
    <details
      className="assembly-guide"
      onToggle={(event) => {
        if (event.currentTarget.open !== open) {
          setChosen(event.currentTarget.open);
        }
      }}
      open={open}
    >
      <summary className="assembly-guide__summary">{text.title}</summary>
      <ol className="assembly-guide__steps">
        {text.steps.map((step) => (
          <li className="assembly-guide__step" key={step.name}>
            <strong>{step.name}</strong> {step.text}
          </li>
        ))}
      </ol>
    </details>
  );
}
