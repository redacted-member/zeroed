import { describe, expect, it } from 'vitest';

import type { JobSetup } from '../../bridge/types';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { materialSummary } from './model';

const text = strings.rightPanel.material;

describe('material summary', () => {
  const base: JobSetup = useDocumentStore.getState().job;

  it('shows the thickness, Z zero, safe Z and home of a flat job', () => {
    const job: JobSetup = {
      ...base,
      units: 'mm',
      kind: 'singleSided',
      thickness: 18,
      zZero: 'bottom',
      machine: { ...base.machine, safeZ: 5, homeX: 0, homeY: 0, homeZ: 20 },
    };
    expect(materialSummary(job)).toEqual([
      { label: text.thickness, value: '18 mm' },
      { label: text.zZero, value: text.machineBed },
      { label: text.safeZ, value: '5 mm' },
      { label: text.home, value: 'X:0 Y:0 Z:20' },
    ]);
  });

  it('shows the diameter and the cylinder Z zero of a rotary job', () => {
    const job: JobSetup = {
      ...base,
      units: 'mm',
      kind: 'rotary',
      rotary: { ...base.rotary, diameter: 50, zZero: 'centre' },
    };
    const rows = materialSummary(job);
    expect(rows[0]).toEqual({ label: text.diameter, value: '50 mm' });
    expect(rows[1]).toEqual({ label: text.zZero, value: text.cylinderCentre });
  });

  it('lists every setting in the detailed view', () => {
    const job: JobSetup = {
      ...base,
      units: 'mm',
      kind: 'singleSided',
      width: 600,
      height: 400,
      thickness: 18,
      zZero: 'top',
      xyOrigin: 'bottomLeft',
      machine: {
        ...base.machine,
        safeZ: 5,
        plungeGap: 2,
        modelGap: 1,
        homeX: 0,
        homeY: 0,
        homeZ: 20,
      },
    };
    expect(materialSummary(job, true)).toEqual([
      { label: text.jobSize, value: 'X:600 mm Y:400 mm' },
      { label: text.thickness, value: '18 mm' },
      { label: text.zZero, value: text.materialSurface },
      { label: text.datum, value: text.datumNames.bottomLeft },
      { label: text.modelGap, value: '1 mm' },
      { label: text.clearance, value: '5 mm' },
      { label: text.plunge, value: '2 mm' },
      { label: text.home, value: 'X:0 Y:0 Z:20' },
    ]);
    const below = { ...job, machine: { ...job.machine, gapBelow: true } };
    expect(materialSummary(below, true)[4]).toEqual({ label: text.modelGapBelow, value: '1 mm' });
    const rotary = { ...below, kind: 'rotary' as const };
    expect(materialSummary(rotary, true)[4]?.label).toBe(text.gapInside);
  });
});
