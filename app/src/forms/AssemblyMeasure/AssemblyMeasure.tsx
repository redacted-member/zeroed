import type { JSX } from 'react';

import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { useAssemblyFitStore } from '../../state/assemblyFitStore';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { measureLines } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { whyStrings } from '../../lib/strings/why';

import './AssemblyMeasure.scss';

/**
 * Measure: Pick Points, then a click on a point of a piece and one on a
 * point of the same or another piece. Each snaps to the nearest face of
 * its piece's exact cut edges; the distance shows here and in the 3D view,
 * with the gap between the faces when they are parallel.
 */
export function AssemblyMeasure(): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const pieces = useAssemblyStore((s) => s.pieces);
  const measure = useAssemblyFitStore((s) => s.measure);
  const startMeasure = useAssemblyFitStore((s) => s.startMeasure);
  const cancelMeasure = useAssemblyFitStore((s) => s.cancelMeasure);
  const lines = measure.result === null ? null : measureLines(measure.result, units);

  const pickWhy = pieces.length === 0 ? whyStrings.why.noPieces : undefined;
  return (
    <fieldset className="assembly-measure">
      <legend className="assembly-measure__heading">{text.measure}</legend>
      <p aria-live="polite" className="assembly-measure__status">
        {measure.step === 'first'
          ? text.pickFirst
          : measure.step === 'second'
            ? text.pickSecond
            : measure.busy
              ? text.measuring
              : text.measureHint}
      </p>
      {lines === null ? null : (
        <ul className="assembly-measure__results">
          {lines.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
      {measure.step === null ? (
        <button
          className="assembly-measure__button"
          disabled={pickWhy !== undefined}
          onClick={startMeasure}
          type="button"
          data-tip={whyTip(text.pickPoints, pickWhy)}
        >
          <ButtonFace icon="measure" label={text.pickPoints} />
        </button>
      ) : (
        <button
          className="assembly-measure__button"
          onClick={cancelMeasure}
          type="button"
          data-tip={text.cancel}
        >
          <ButtonFace icon="dismiss" label={text.cancel} />
        </button>
      )}
    </fieldset>
  );
}
