import { useId, type JSX } from 'react';

import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { formatLength, unitSuffix } from '../../lib/units';
import { useAssemblyFitStore } from '../../state/assemblyFitStore';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import type { SectionAxis } from '../../bridge/assemblyFit';
import { sectionRange } from '../../views/View3D/sectionPlane';

import './AssemblySection.scss';

const AXES: readonly SectionAxis[] = ['x', 'y', 'z'];

/**
 * Section: a plane across X, Y or Z cuts the pieces in the 3D view and
 * caps the cut faces, so the gap of a joint can be seen; the offset slider
 * drags the plane through the assembly, Flip keeps the other side.
 */
export function AssemblySection(): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const pieces = useAssemblyStore((s) => s.pieces);
  const section = useAssemblyFitStore((s) => s.section);
  const setSection = useAssemblyFitStore((s) => s.setSection);
  const name = useId();
  const onId = useId();
  const flipId = useId();
  const sliderId = useId();
  const range = sectionRange(pieces, section.axis);
  const [lo, hi] = range ?? [0, 0];
  const offset = Math.min(hi, Math.max(lo, section.offset ?? (lo + hi) / 2));
  const step = Math.max((hi - lo) / 1000, 0.01);

  return (
    <fieldset className="assembly-section">
      <legend className="assembly-section__heading">{text.section}</legend>
      <div className="assembly-section__check">
        <input
          checked={section.on}
          disabled={range === null}
          id={onId}
          onChange={(event) => setSection({ on: event.target.checked })}
          type="checkbox"
        />
        <label htmlFor={onId}>{text.sectionOn}</label>
      </div>
      <div className="assembly-section__axes" role="radiogroup" aria-label={text.sectionAxis}>
        {AXES.map((axis) => {
          const id = `${name}-${axis}`;
          return (
            <div className="assembly-section__check" key={axis}>
              <input
                checked={section.axis === axis}
                id={id}
                name={name}
                onChange={() => setSection({ axis, offset: null })}
                type="radio"
              />
              <label htmlFor={id}>{axis.toUpperCase()}</label>
            </div>
          );
        })}
      </div>
      <div className="assembly-section__row">
        <label className="assembly-section__label" htmlFor={sliderId}>
          {text.sectionOffset}
        </label>
        <input
          className="assembly-section__slider"
          disabled={!section.on || range === null}
          id={sliderId}
          max={hi}
          min={lo}
          onChange={(event) => setSection({ offset: Number(event.target.value) })}
          step={step}
          type="range"
          value={offset}
        />
        <output className="assembly-section__value" htmlFor={sliderId}>
          {`${formatLength(offset, units)} ${unitSuffix(units)}`}
        </output>
      </div>
      <div className="assembly-section__check">
        <input
          checked={section.flip}
          disabled={!section.on}
          id={flipId}
          onChange={(event) => setSection({ flip: event.target.checked })}
          type="checkbox"
        />
        <label htmlFor={flipId}>{text.sectionFlip}</label>
      </div>
    </fieldset>
  );
}
