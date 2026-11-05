import type { JSX } from 'react';

import { strings } from '../../lib/strings';
import { formatLength, unitSuffix } from '../../lib/units';
import { useBusyStore } from '../../state/busyStore';
import { useDocumentStore } from '../../state/documentStore';
import { useDrawStore } from '../../state/drawStore';
import { useFaceEditStore } from '../../state/faceEditStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { statusLine } from './model';
import { usePanelUnder } from './usePanelUnder';

import './StatusBar.scss';

export function StatusBar(): JSX.Element {
  const job = useDocumentStore((state) => state.job);
  const kernelStatus = useDocumentStore((state) => state.kernelStatus);
  const cursor = useViewStore((state) => state.cursor);
  const snapHit = useViewStore((state) => state.snapHit);
  const selectedCount = useDocumentStore((state) => state.selection.ids.length);
  const lastError = useDocumentStore((state) => state.lastError);
  const statusHint = useUiStore((state) => state.statusHint);
  const showUsage = usePrefsStore((state) => state.prefs.general.showUsageInfo);
  const hint = showUsage ? statusHint : null;
  const quickText = useDrawStore((state) => state.quickText);
  const units = unitSuffix(job.units);
  const busy = useBusyStore((state) => state.busy);
  const progress = useBusyStore((state) => state.progress);
  const stopping = useBusyStore((state) => state.stopping);
  const panel = useFaceEditStore((state) => state.pointerPanel);
  usePanelUnder();

  return (
    <footer aria-label={strings.statusBar.label} className="status-bar">
      {busy ? (
        <span className="status-bar__item status-bar__busy">
          <span>{stopping ? strings.statusBar.stopping : strings.statusBar.working}</span>
          <progress
            aria-label={strings.statusBar.progress}
            className="status-bar__progress"
            max={1}
            {...(progress === null ? {} : { value: progress })}
          />
        </span>
      ) : (
        <span className="status-bar__item status-bar__item--line" data-status={kernelStatus}>
          {statusLine(kernelStatus, hint)}
        </span>
      )}
      <span className="status-bar__item">
        {snapHit === null ? '' : `${strings.statusBar.snap}: ${strings.snap[snapHit.kind]}`}
      </span>
      {quickText === '' ? null : (
        <span aria-live="polite" className="status-bar__item status-bar__item--quick">
          {`${strings.statusBar.quickKeys}: ${quickText}`}
        </span>
      )}
      {lastError === null ? null : (
        <span className="status-bar__item status-bar__item--error" role="alert">
          {lastError}
        </span>
      )}
      <span className="status-bar__spacer" />
      {panel === null ? null : <span className="status-bar__item">{panel}</span>}
      <span className="status-bar__item">{strings.statusBar.selected(selectedCount)}</span>
      <span className="status-bar__item">
        {cursor === null
          ? ''
          : `X ${formatLength(cursor.x, job.units)}  Y ${formatLength(cursor.y, job.units)}`}
      </span>
      <span className="status-bar__item">{`${strings.statusBar.units}: ${units}`}</span>
    </footer>
  );
}
