// The toolpath preview's checks in the 3D view: the gouge, leftover and
// undercut colors on the finished preview (hidden while the animation runs,
// since they belong to the finished stock) and a marker at each issue, the
// one picked in the Preview Toolpaths list larger. A rotary job shows both
// on the blank wrapped round its axis.

import { useEffect, type RefObject } from 'react';

import type { Heightfield } from '../../bridge/cam';
import type { JobSetup } from '../../bridge/types';
import { checkFits, previewIssues } from '../../forms/PreviewToolpathsForm/issues';
import { isRotary } from '../../lib/rotary';
import { usePreviewStore } from '../../state/previewStore';
import type { View3DRenderer } from './renderer';
import { toolScenePosition } from './toolMarker';
import { wrappedScenePoint } from './wrappedMesh';

/** Marker size as a fraction of the job's longer side, and the least, mm. */
const MARKER_SCALE = 0.003;
const MARKER_MIN = 0.4;

export function usePreviewChecks(
  rendererRef: RefObject<View3DRenderer | null>,
  scheduleDraw: () => void,
  job: JobSetup,
  preview: Heightfield | null,
  surfaceZ: number,
): void {
  const check = usePreviewStore((state) => state.check);
  const showDeviation = usePreviewStore((state) => state.showDeviation);
  const animating = usePreviewStore((state) => state.animating);
  const collisions = usePreviewStore((state) => state.collisions);
  const selected = usePreviewStore((state) => state.selectedIssue);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const rotary = isRotary(job);
    const painted = showDeviation && !animating && checkFits(check, preview);
    renderer.setCheck(painted && check !== null ? check.map : null);
    const issues = showDeviation && preview !== null ? previewIssues(collisions, check) : [];
    const picked = issues.findIndex((issue) => issue.key === selected);
    renderer.setIssueMarkers(
      issues.map((issue) => ({
        at:
          rotary && preview !== null
            ? wrappedScenePoint(job, preview.top, issue)
            : toolScenePosition(issue, surfaceZ),
        severity: issue.severity,
      })),
      picked < 0 ? null : picked,
      Math.max(Math.max(job.width, job.height) * MARKER_SCALE, MARKER_MIN),
    );
    scheduleDraw();
  }, [
    animating,
    check,
    collisions,
    job,
    preview,
    rendererRef,
    scheduleDraw,
    selected,
    showDeviation,
    surfaceZ,
  ]);
}
