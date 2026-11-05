import { lazy, Suspense, type JSX } from 'react';

import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { materialSummary } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './MaterialSummary.scss';

// The pictures load on demand, keeping their drawings out of the start bundle.
const MaterialSummaryPictures = lazy(() =>
  import('../MaterialSummaryPictures/MaterialSummaryPictures').then((module) => ({
    default: module.MaterialSummaryPictures,
  })),
);

/**
 * The Material Setup section at the top of the Toolpaths panel: the
 * material the toolpaths are cut from, with Set... opening Material Setup.
 */
export function MaterialSummary(): JSX.Element {
  const job = useDocumentStore((store) => store.job);
  const openForm = useUiStore((store) => store.openForm);
  const detailed = usePrefsStore((store) => store.prefs.toolpaths.detailedMaterialSummary);
  const text = strings.rightPanel.material;
  return (
    <section aria-labelledby="material-summary-title" className="material-summary">
      <div className="material-summary__head">
        <h3 className="material-summary__title" id="material-summary-title">
          {text.title}
        </h3>
        <button
          className="material-summary__set"
          onClick={() => openForm('materialSetup')}
          type="button"
          data-tip={text.set}
        >
          <ButtonFace icon="materialSetup" label={text.set} />
        </button>
      </div>
      <div className="material-summary__body">
        <Suspense fallback={null}>
          <MaterialSummaryPictures job={job} side="left" />
        </Suspense>
        <dl className="material-summary__rows">
          {materialSummary(job, detailed).map((row) => (
            <div className="material-summary__row" key={row.label}>
              <dt className="material-summary__label">{row.label}</dt>
              <dd className="material-summary__value">{row.value}</dd>
            </div>
          ))}
        </dl>
        <Suspense fallback={null}>
          <MaterialSummaryPictures job={job} side="right" />
        </Suspense>
      </div>
    </section>
  );
}
