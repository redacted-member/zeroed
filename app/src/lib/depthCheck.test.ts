import { describe, expect, it } from 'vitest';

import type { JobSetup } from '../bridge/types';
import { useDocumentStore } from '../state/documentStore';
import { depthsMayChange, depthWarning } from './depthCheck';

describe('depthWarning', () => {
  it('says nothing when every toolpath fits the material', () => {
    expect(depthWarning({ all: false, toolpaths: [] }, 'mm')).toBeNull();
  });

  it('names each toolpath with its depth and the material thickness', () => {
    const text = depthWarning(
      {
        all: false,
        toolpaths: [
          { id: 1, name: 'Profile 1', depth: 12, thickness: 10 },
          { id: 2, name: 'Pocket 1', depth: 11.5, thickness: 10 },
        ],
      },
      'mm',
    );
    expect(text).toContain('2 toolpaths cut deeper below the material than set');
    expect(text).toContain('(visible toolpaths checked)');
    expect(text).toContain('Profile 1 (12 mm deep, material 10 mm)');
    expect(text).toContain('Pocket 1 (11.5 mm deep');
  });

  it('shows inches in an inch job and drops the visible note when all were checked', () => {
    const text = depthWarning(
      { all: true, toolpaths: [{ id: 1, name: 'Cut Out', depth: 25.4, thickness: 19.05 }] },
      'inch',
    );
    expect(text).toContain(
      'A toolpath cuts deeper below the material than set after the job change:',
    );
    expect(text).toContain('Cut Out (1 inch deep, material 0.75 inch)');
  });
});

describe('depthsMayChange', () => {
  const job: JobSetup = useDocumentStore.getState().job;

  it('skips changes that move or resize the job only', () => {
    expect(depthsMayChange(job, { ...job, xyOrigin: 'centre', offsetX: 5 })).toBe(false);
    expect(depthsMayChange(job, { ...job, width: job.width * 2, units: 'inch' })).toBe(false);
  });

  it('checks changes of thickness, Z zero or model position', () => {
    expect(depthsMayChange(job, { ...job, thickness: job.thickness - 1 })).toBe(true);
    expect(depthsMayChange(job, { ...job, zZero: job.zZero === 'top' ? 'bottom' : 'top' })).toBe(
      true,
    );
    expect(depthsMayChange(job, { ...job, machine: { ...job.machine, modelGap: 2 } })).toBe(
      job.machine.modelGap !== 2,
    );
  });
});
