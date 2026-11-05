// Verify Fit in the 3D view: the picked joint's faces, or with Show Fit
// every joint's, colored by how they fit and following the pieces as they
// explode; picking a joint frames it. Also the section plane cutting the
// pieces and the measurement between two picked points.

import { useEffect, useMemo, type RefObject } from 'react';

import { pieceKey, type Triple } from '../../bridge/assembly';
import { measureLabel } from '../../forms/AssemblyMeasure/model';
import { useAssemblyFitStore } from '../../state/assemblyFitStore';
import { isShown, useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import type { OrbitView } from './camera3d';
import { explodedPieces, explodeMoves } from './explode';
import { fitMeshes, jointFrame } from './fitOverlay';
import type { View3DRenderer } from './renderer';
import { sectionPlane, sectionRange } from './sectionPlane';

export function useAssemblyFit(
  rendererRef: RefObject<View3DRenderer | null>,
  scheduleDraw: () => void,
  goTo: (view: OrbitView) => void,
): void {
  const active = useAssemblyStore((s) => s.active);
  const pieces = useAssemblyStore((s) => s.pieces);
  const isolated = useAssemblyStore((s) => s.isolated);
  const joints = useAssemblyFitStore((s) => s.joints);
  const picked = useAssemblyFitStore((s) => s.picked);
  const showFit = useAssemblyFitStore((s) => s.showFit);
  const offsets = useAssemblyFitStore((s) => s.offsets);
  const explode = useAssemblyFitStore((s) => s.explode);
  const section = useAssemblyFitStore((s) => s.section);
  const measure = useAssemblyFitStore((s) => s.measure);
  const units = useDocumentStore((s) => s.job.units);
  const steps = useAssemblyStore((s) => s.explodeSteps);
  const moves = useMemo(() => explodeMoves(offsets, explode, steps), [explode, offsets, steps]);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    // Joint faces on pieces that do not show stay hidden with them.
    const shown = explodedPieces(
      pieces.filter((p) => isShown(p, isolated)),
      moves,
      1,
    );
    renderer.setAssemblyFit(active ? fitMeshes(joints, shown, picked, showFit) : []);
    scheduleDraw();
  }, [active, isolated, joints, moves, picked, pieces, rendererRef, scheduleDraw, showFit]);

  // The section plane: across the pieces as assembled, the middle unless moved.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const range = sectionRange(pieces, section.axis);
    if (!active || !section.on || range === null) {
      renderer.setAssemblySection(null);
    } else {
      const offset = Math.min(
        range[1],
        Math.max(range[0], section.offset ?? (range[0] + range[1]) / 2),
      );
      renderer.setAssemblySection(sectionPlane(section.axis, offset, section.flip));
    }
    scheduleDraw();
  }, [active, pieces, rendererRef, scheduleDraw, section]);

  // The measurement, its ends moving with their pieces as they explode.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const result = measure.result;
    if (!active || result === null || measure.a === null || measure.b === null) {
      renderer.setAssemblyMeasure(null);
    } else {
      const moved = (point: Triple, key: string): Triple => {
        const o = moves[key] ?? [0, 0, 0];
        return [point[0] + o[0], point[1] + o[1], point[2] + o[2]];
      };
      renderer.setAssemblyMeasure({
        a: moved(result.a.point, pieceKey(measure.a)),
        b: moved(result.b.point, pieceKey(measure.b)),
        text: measureLabel(result, units),
      });
    }
    scheduleDraw();
  }, [active, measure, moves, rendererRef, scheduleDraw, units]);

  // A joint picked in the list is framed, once.
  useEffect(() => {
    const renderer = rendererRef.current;
    const joint = picked === null ? undefined : joints?.[picked];
    if (renderer === null || joint === undefined) {
      return;
    }
    const { offsets, explode: share } = useAssemblyFitStore.getState();
    const moved = explodeMoves(offsets, share, useAssemblyStore.getState().explodeSteps);
    const zero: Triple = [0, 0, 0];
    const a = moved[pieceKey(joint.a)] ?? zero;
    const b = moved[pieceKey(joint.b)] ?? zero;
    const shift: Triple = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2];
    goTo(renderer.framedOn(jointFrame(joint, shift)));
  }, [goTo, joints, picked, rendererRef]);
}
