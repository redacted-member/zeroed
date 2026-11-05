import { useId, useState, type JSX } from 'react';

import { pieceKey, type PieceRef } from '../../bridge/assembly';
import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { formatLength, parseLength, toMillimetres, unitSuffix } from '../../lib/units';
import { fitTarget, useAssemblyFitStore } from '../../state/assemblyFitStore';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { pieceLabel } from '../AssemblyViewForm/model';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { assemblyFit } from '../HelperPicture/pictures/furniture';
import { HelperRow } from '../HelperRow/HelperRow';
import { NumberField } from '../NumberField/NumberField';
import { jointLines, jointOrder, problemCount } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { whyStrings } from '../../lib/strings/why';

import './AssemblyFit.scss';

/**
 * The Assembly View's Fit section: a target clearance and tolerance, and
 * Verify Fit listing every joint between the placed pieces worst first,
 * with each side's gap, any overlap and what it was measured on. A click
 * on a joint shows its faces colored in the 3D view and frames it; Show
 * Fit colors every joint at once.
 */
export function AssemblyFit(): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const pieces = useAssemblyStore((s) => s.pieces);
  const target = useAssemblyFitStore((s) => s.target);
  const tolerance = useAssemblyFitStore((s) => s.tolerance);
  const joints = useAssemblyFitStore((s) => s.joints);
  const verifying = useAssemblyFitStore((s) => s.verifying);
  const error = useAssemblyFitStore((s) => s.error);
  const picked = useAssemblyFitStore((s) => s.picked);
  const showFit = useAssemblyFitStore((s) => s.showFit);
  const includeHidden = useAssemblyFitStore((s) => s.includeHidden);
  const setIncludeHidden = useAssemblyFitStore((s) => s.setIncludeHidden);
  const setTarget = useAssemblyFitStore((s) => s.setTarget);
  const setTolerance = useAssemblyFitStore((s) => s.setTolerance);
  const verify = useAssemblyFitStore((s) => s.verify);
  const pick = useAssemblyFitStore((s) => s.pick);
  const setShowFit = useAssemblyFitStore((s) => s.setShowFit);
  const start = fitTarget({ target, tolerance }, units);
  const [targetText, setTargetText] = useState(() => formatLength(start.target, units));
  const [toleranceText, setToleranceText] = useState(() => formatLength(start.tolerance, units));
  const showId = useId();
  const hiddenId = useId();
  const listId = useId();
  const unit = unitSuffix(units);
  const valid = (value: string) => {
    const typed = parseLength(value);
    return typed !== null && Number.isFinite(typed) && typed >= 0;
  };
  const ready = valid(targetText) && valid(toleranceText);
  const verifyWhy = firstWhy(
    [verifying, whyStrings.why.busy],
    [pieces.length < 2, whyStrings.why.twoPieces],
    [!ready, whyStrings.why.fixValues],
  );
  const nameOf = (ref: PieceRef) => {
    const row = pieces.find((p) => pieceKey(p) === pieceKey(ref));
    return row === undefined ? pieceKey(ref) : pieceLabel(row);
  };
  const run = () => {
    if (ready && !verifying) {
      void verify();
    }
  };
  const field = (
    label: string,
    value: string,
    setText: (next: string) => void,
    setMm: (mm: number) => void,
  ) => (
    <NumberField
      error={valid(value) ? undefined : text.badValue}
      label={label}
      onChange={(next) => {
        setText(next);
        const typed = parseLength(next);
        if (typed !== null && Number.isFinite(typed) && typed >= 0) {
          setMm(toMillimetres(typed, units));
        }
      }}
      onEnter={run}
      unit={unit}
      value={value}
    />
  );

  return (
    <fieldset className="assembly-fit">
      <legend className="assembly-fit__heading">{text.fit}</legend>
      <p className="assembly-fit__hint">{text.fitHint}</p>
      <HelperRow picture={<HelperPicture picture={assemblyFit} />}>
        {field(text.target, targetText, setTargetText, setTarget)}
        {field(text.tolerance, toleranceText, setToleranceText, setTolerance)}
      </HelperRow>
      <div className="assembly-fit__check" data-tip={text.includeHiddenHint}>
        <input
          checked={includeHidden}
          id={hiddenId}
          onChange={(event) => setIncludeHidden(event.target.checked)}
          type="checkbox"
        />
        <label htmlFor={hiddenId}>{text.includeHidden}</label>
      </div>
      <div className="assembly-fit__row">
        <button
          className="assembly-fit__button"
          disabled={verifyWhy !== undefined}
          onClick={run}
          type="button"
          data-tip={whyTip(verifying ? text.verifying : text.verify, verifyWhy)}
        >
          <ButtonFace icon="validator" label={verifying ? text.verifying : text.verify} />
        </button>
        <div className="assembly-fit__check">
          <input
            checked={showFit}
            disabled={joints === null || joints.length === 0}
            id={showId}
            onChange={(event) => setShowFit(event.target.checked)}
            type="checkbox"
          />
          <label htmlFor={showId}>{text.showFit}</label>
        </div>
      </div>
      {joints === null ? null : joints.length === 0 ? (
        <p aria-live="polite" className="assembly-fit__status">
          {text.noJoints}
        </p>
      ) : (
        <>
          <p aria-live="polite" className="assembly-fit__status" id={listId}>
            {text.summary(joints.length, problemCount(joints))}
          </p>
          <ul aria-labelledby={listId} className="assembly-fit__joints">
            {jointOrder(joints).map((index) => {
              const joint = joints[index];
              if (joint === undefined) {
                return null;
              }
              const lines = jointLines(joint, nameOf, units);
              return (
                <li key={index}>
                  <button
                    aria-pressed={picked === index}
                    className={`assembly-fit__joint assembly-fit__joint--${joint.status}`}
                    onClick={() => pick(picked === index ? null : index)}
                    data-tip={text.showJoint(lines.title)}
                    type="button"
                  >
                    <span className="assembly-fit__joint-head">
                      <span className="assembly-fit__joint-name">{lines.title}</span>
                      <span className="assembly-fit__badge">{lines.status}</span>
                    </span>
                    {lines.sides.length > 0 ? (
                      <span className="assembly-fit__sides">{lines.sides.join(', ')}</span>
                    ) : null}
                    {lines.overlap === null ? null : (
                      <span className="assembly-fit__overlap">{lines.overlap}</span>
                    )}
                    <span className="assembly-fit__source">{lines.source}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      )}
      {error === null ? null : (
        <p className="assembly-fit__error" role="alert">
          {error}
        </p>
      )}
    </fieldset>
  );
}
