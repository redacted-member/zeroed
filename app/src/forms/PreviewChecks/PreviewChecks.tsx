import { useEffect, useState, type JSX } from 'react';

import type { IssueKind } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import {
  formatDisplay,
  formatLength,
  parseLength,
  toMillimetres,
  unitSuffix,
} from '../../lib/units';
import { usePreviewStore } from '../../state/previewStore';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { tipText } from '../../shell/Tooltip/model';
import { CHECK_COLOURS } from '../../views/View3D/checkColours';
import { IconButton } from '../IconButton/IconButton';
import { NumberField } from '../NumberField/NumberField';
import { LEGEND_KINDS, previewIssues, severityCounts } from '../PreviewToolpathsForm/issues';
import type { Units } from '../../bridge/types';

import './PreviewChecks.scss';

const text = strings.toolpaths.preview;

/** Most issues listed. */
const LISTED = 60;
/** How long the tolerance waits for typing to stop before it checks again, ms. */
const TYPING_MS = 400;

const KIND_ICONS: Readonly<Record<IssueKind, string>> = {
  rapid: 'issueRapid',
  plunge: 'issuePlunge',
  flute: 'issueFlute',
  shank: 'issueShank',
  holder: 'issueHolder',
  clamp: 'issueClamp',
  gouge: 'issueGouge',
  leftover: 'issueLeftover',
  undercut: 'issueUndercut',
};

/** Square millimetres as the job's unit shows them. */
function areaText(mm2: number, units: Units): string {
  return units === 'inch'
    ? text.areaInch(formatDisplay(mm2 / (25.4 * 25.4), units))
    : text.areaMm(formatDisplay(mm2, units));
}

interface PreviewChecksProps {
  units: Units;
  /** Toolpath names by id. */
  names: ReadonlyMap<number, string>;
}

/** The Preview Toolpaths checks: the colors toggle, tolerance, legend and
 * the issues found, each of which shows itself in the 3D view. */
/** Give it `key={units}` so the tolerance shows again in a new unit. */
export function PreviewChecks({ units, names }: PreviewChecksProps): JSX.Element {
  useExtraIcons();
  const collisions = usePreviewStore((store) => store.collisions);
  const check = usePreviewStore((store) => store.check);
  const checking = usePreviewStore((store) => store.checking);
  const showDeviation = usePreviewStore((store) => store.showDeviation);
  const setShowDeviation = usePreviewStore((store) => store.setShowDeviation);
  const tolerance = usePreviewStore((store) => store.tolerance);
  const setTolerance = usePreviewStore((store) => store.setTolerance);
  const selected = usePreviewStore((store) => store.selectedIssue);
  const goToIssue = usePreviewStore((store) => store.goToIssue);
  const shown = usePreviewStore((store) => store.heightfield !== null);
  const [typed, setTyped] = useState(() => formatLength(tolerance, units));

  // A new tolerance checks the preview again once typing stops.
  useEffect(() => {
    const value = parseLength(typed);
    if (value === null || value <= 0) {
      return;
    }
    const timer = window.setTimeout(() => setTolerance(toMillimetres(value, units)), TYPING_MS);
    return () => window.clearTimeout(timer);
  }, [setTolerance, typed, units]);

  const issues = previewIssues(collisions, check);
  const counts = severityCounts(issues);
  const hidden = check === null ? 0 : Math.max(check.found - check.issues.length, 0);
  const value = parseLength(typed);
  const error = value === null || value <= 0 ? text.toleranceError : undefined;

  return (
    <fieldset className="preview-checks">
      <legend className="preview-checks__legend">{text.checks}</legend>
      <div className="preview-checks__row">
        <div className="preview-checks__toggle">
          <IconButton
            description={text.checkColoursTip}
            icon="checkColors"
            label={text.checkColours}
            onClick={() => setShowDeviation(!showDeviation)}
            pressed={showDeviation}
            title={text.checkColoursTitle}
          />
        </div>
        <div
          className="preview-checks__tolerance"
          data-tip={tipText(text.tolerance, undefined, text.toleranceTip)}
        >
          <span aria-hidden="true" className="preview-checks__icon">
            {toolIcon('tolerance')}
          </span>
          <NumberField
            error={error}
            label={text.tolerance}
            onChange={setTyped}
            unit={unitSuffix(units)}
            value={typed}
          />
        </div>
      </div>
      <ul aria-label={text.legendLabel} className="preview-checks__key">
        {LEGEND_KINDS.map((kind) => (
          <li className="preview-checks__key-item" data-tip={text.kindTips[kind]} key={kind}>
            <svg aria-hidden="true" className="preview-checks__swatch" viewBox="0 0 10 10">
              <rect fill={CHECK_COLOURS[kind]} height="10" rx="2" width="10" />
            </svg>
            {text.kinds[kind]}
          </li>
        ))}
      </ul>
      <div aria-live="polite" className="preview-checks__status">
        {checking ? <p className="preview-checks__hint">{text.checking}</p> : null}
        {shown && !checking && check !== null && issues.length === 0 ? (
          <p className="preview-checks__hint">{text.noIssues}</p>
        ) : null}
        {issues.length > 0 ? (
          <p className="preview-checks__summary">
            {text.summary(counts.high, counts.medium, counts.low)}
          </p>
        ) : null}
      </div>
      {issues.length > 0 ? (
        <ul aria-label={text.issues} className="preview-checks__issues">
          {issues.slice(0, LISTED).map((issue) => (
            <li key={issue.key}>
              <button
                aria-current={issue.key === selected ? 'true' : undefined}
                className={`preview-checks__issue preview-checks__issue--${issue.severity}`}
                data-tip={tipText(
                  text.kinds[issue.kind],
                  undefined,
                  `${text.kindTips[issue.kind]} ${text.goTo}.`,
                )}
                onClick={() => void goToIssue(issue)}
                type="button"
              >
                <span aria-hidden="true" className="preview-checks__icon">
                  {toolIcon(KIND_ICONS[issue.kind])}
                </span>
                <span className="preview-checks__issue-text">
                  <span className="preview-checks__issue-head">
                    <span className="preview-checks__issue-kind">{text.kinds[issue.kind]}</span>
                    <span className="preview-checks__severity">
                      {text.severities[issue.severity]}
                    </span>
                  </span>
                  <span className="preview-checks__issue-where">
                    {text.where(
                      names.get(issue.toolpathId) ?? String(issue.toolpathId),
                      formatLength(issue.x, units),
                      formatLength(issue.y, units),
                      formatLength(issue.z, units),
                    )}
                  </span>
                  {issue.depth !== null && issue.area !== null ? (
                    <span className="preview-checks__issue-where">
                      {text.offBy(
                        `${formatLength(issue.depth, units)} ${unitSuffix(units)}`,
                        areaText(issue.area, units),
                      )}
                    </span>
                  ) : null}
                </span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {hidden > 0 ? <p className="preview-checks__hint">{text.moreAreas(hidden)}</p> : null}
    </fieldset>
  );
}
