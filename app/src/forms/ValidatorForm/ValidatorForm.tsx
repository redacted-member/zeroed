import { useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { ValidationIssue } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { useOverlays } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { PreviewOverlay } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  hasZeroLength,
  issueBounds,
  issueKey,
  MARK_KINDS,
  MARK_TONE,
  markKind,
  markPoints,
  markShape,
  type MarkKind,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './ValidatorForm.scss';

const text = strings.editObjects.validator;

/** Half the size of a mark, in screen pixels. */
const MARK_PX = 5;

/** The legend's picture of a mark: the same shapes the view draws. */
function MarkIcon({ kind }: { kind: MarkKind }): JSX.Element {
  const body =
    kind === 'open' ? (
      <rect height="8" width="8" x="2" y="2" />
    ) : kind === 'intersection' ? (
      <path d="M2 2l8 8M10 2l-8 8" />
    ) : kind === 'duplicate' ? (
      <path d="M6 1l5 5-5 5-5-5z" />
    ) : (
      <path d="M2 2h8l-4 8z" />
    );
  return (
    <svg
      aria-hidden="true"
      className={`validator-form__mark validator-form__mark--${MARK_TONE[kind]}`}
      viewBox="0 0 12 12"
    >
      {body}
    </svg>
  );
}

export function ValidatorForm(): JSX.Element {
  const validateSelection = useDocumentStore((store) => store.validateSelection);
  const selectedCount = useDocumentStore((store) => store.selection.ids.length);
  const pxPerMm = useViewStore((store) => store.camera.pxPerMm);
  const closeForm = useUiStore((store) => store.closeForm);
  const [issues, setIssues] = useState<ValidationIssue[] | null>(null);
  const [vgroove, setVgroove] = useState(false);
  const [searching, setSearching] = useState(false);
  // Each search gets a number; Stop moves it on, so an answer that comes
  // back after Stop is dropped.
  const search = useRef(0);

  const run = () => {
    const mine = search.current + 1;
    search.current = mine;
    setSearching(true);
    void validateSelection(vgroove).then((found) => {
      if (search.current === mine) {
        setIssues(found);
        setSearching(false);
      }
    });
  };

  const stop = () => {
    search.current += 1;
    setSearching(false);
  };

  const fix = () => {
    const ids = [...new Set((issues ?? []).map((issue) => issue.id))];
    void api.ops
      .fixZeroLength(ids)
      .then(() => useDocumentStore.getState().refresh())
      .then(run);
  };

  const show = (issue: ValidationIssue) => {
    const doc = useDocumentStore.getState();
    const box = issueBounds(issue, doc.entities);
    void doc.select([issue.id]);
    useViewStore.getState().zoomToBounds(box);
  };

  const overlays = useMemo<PreviewOverlay[]>(() => {
    const r = MARK_PX / pxPerMm;
    return (issues ?? []).flatMap((issue) => {
      const kind = markKind(issue);
      return markPoints(issue).map((at) => ({
        path: markShape(kind, at, r),
        tone: MARK_TONE[kind],
      }));
    });
  }, [issues, pxPerMm]);
  useOverlays(overlays);

  const label = searching ? text.stop : selectedCount > 0 ? text.searchSelected : text.searchAll;
  const fixWhy = firstWhy(
    [issues === null, strings.why.checkFirst],
    [issues !== null && !hasZeroLength(issues), strings.why.noZeroLength],
  );
  return (
    <div className="validator-form">
      <p className="validator-form__hint">{text.hint}</p>
      <CheckField checked={vgroove} label={text.vgroove} onChange={setVgroove} />
      <button
        className="validator-form__button"
        onClick={searching ? stop : run}
        type="button"
        data-tip={label}
      >
        <ButtonFace icon="validator" label={label} />
      </button>
      <fieldset className="validator-form__group">
        <legend className="validator-form__legend">{text.marks}</legend>
        <ul className="validator-form__legend-list">
          {MARK_KINDS.map((kind) => (
            <li className="validator-form__legend-row" key={kind}>
              <MarkIcon kind={kind} />
              <span>{text.markNames[kind]}</span>
            </li>
          ))}
        </ul>
      </fieldset>
      <button
        className="validator-form__button"
        disabled={fixWhy !== undefined}
        onClick={fix}
        type="button"
        data-tip={whyTip(text.fixZeroLength, fixWhy)}
      >
        <ButtonFace icon="editDelete" label={text.fixZeroLength} />
      </button>
      {issues === null ? null : (
        <section aria-live="polite" className="validator-form__group">
          <p className="validator-form__summary">
            {issues.length === 0 ? text.none : text.found(issues.length)}
          </p>
          {issues.length === 0 ? null : (
            <ul className="validator-form__list">
              {issues.map((issue, index) => (
                <li className="validator-form__issue" key={issueKey(issue, index)}>
                  <span>{issue.message}</span>
                  <button
                    className="validator-form__show"
                    onClick={() => show(issue)}
                    type="button"
                    data-tip={text.zoom}
                  >
                    <ButtonFace icon="zoomSelection" label={text.zoom} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
