import { useId, type JSX } from 'react';

import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { useAssemblyFitStore } from '../../state/assemblyFitStore';
import { useAssemblyStore } from '../../state/assemblyStore';
import { AssemblyExplodeSteps } from '../AssemblyExplodeSteps/AssemblyExplodeSteps';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { assemblyExplode } from '../HelperPicture/pictures/furniture';
import { HelperRow } from '../HelperRow/HelperRow';

import './AssemblyExplode.scss';

/**
 * The Explode slider: 0 has the pieces assembled, 100% moves each out of
 * its joints along the way they go together, or plays the named explode
 * steps in order when the job has some. It only changes the view (no undo
 * step); the handles hide while the pieces are apart.
 */
export function AssemblyExplode(): JSX.Element {
  const explode = useAssemblyFitStore((s) => s.explode);
  const setExplode = useAssemblyFitStore((s) => s.setExplode);
  const stepped = useAssemblyStore((s) => s.explodeSteps.length > 0);
  const loading = useAssemblyFitStore((s) => s.verifying && s.measured === null) && !stepped;
  const pieces = useAssemblyStore((s) => s.pieces);
  const sliderId = useId();
  const hintId = useId();
  const percent = Math.round(explode * 100);

  return (
    <fieldset className="assembly-explode">
      <legend className="assembly-explode__heading">{text.explode}</legend>
      <p className="assembly-explode__hint" id={hintId}>
        {pieces.length < 2 ? text.explodeNeedsPieces : text.explodeHint}
      </p>
      <HelperRow picture={<HelperPicture picture={assemblyExplode} />}>
        <div className="assembly-explode__row">
          <label className="assembly-explode__label" htmlFor={sliderId}>
            {text.explode}
          </label>
          <input
            aria-describedby={hintId}
            aria-valuetext={text.explodeValue(percent)}
            className="assembly-explode__slider"
            disabled={pieces.length < 2}
            id={sliderId}
            max={100}
            min={0}
            onChange={(event) => setExplode(Number(event.target.value) / 100)}
            step={1}
            type="range"
            value={percent}
          />
          <output className="assembly-explode__value" htmlFor={sliderId}>
            {text.explodeValue(percent)}
          </output>
        </div>
      </HelperRow>
      {loading && explode > 0 ? (
        <p aria-live="polite" className="assembly-explode__status">
          {text.explodeLoading}
        </p>
      ) : null}
      <AssemblyExplodeSteps />
    </fieldset>
  );
}
