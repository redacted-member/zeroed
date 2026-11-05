import { afterEach, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import { DEFAULT_JOB, type JobSetup } from '../bridge/types';
import { useDocumentStore } from '../state/documentStore';
import { pendingPath, usePendingFileStore } from '../state/pendingFileStore';
import { useUiStore } from '../state/uiStore';
import { fakeKernel } from '../testing/fakeKernel';
import {
  closeRequested,
  jobForVectors,
  newJob,
  openFile,
  openFilters,
  openKind,
  openVectorsAsJob,
} from './fileCommands';

/** A kernel holding one job, which a vector import fills with a 60 x 30 drawing. */
function vectorKernel(): void {
  let job: JobSetup = { ...DEFAULT_JOB };
  let imported = false;
  setKernelForTests(
    fakeKernel((name, args) => {
      switch (name) {
        case 'doc.new':
          job = { ...DEFAULT_JOB };
          imported = false;
          return { revision: 1 };
        case 'doc.info':
          return {
            revision: 1,
            path: null,
            dirty: imported,
            activeLayer: 1,
            activeSheet: 1,
            counts: { entities: imported ? 1 : 0, components: 0, toolpaths: 0 },
          };
        case 'job.get':
          return job;
        case 'job.set':
          job = args as JobSetup;
          return job;
        case 'doc.open':
          job = { ...DEFAULT_JOB, width: 500 };
          return { revision: 3 };
        case 'io.importVectors':
          imported = true;
          return { revision: 2, ids: [7], warnings: [] };
        case 'selection.get':
          return imported ? { ids: [7], bbox: [0, 0, 60, 30] } : { ids: [], bbox: null };
        case 'undo.info':
          return { canUndo: imported, canRedo: false, undoLabel: null, redoLabel: null };
        case 'layers.list':
        case 'sheets.list':
        case 'entities.list':
          return [];
        default:
          throw new Error(`unexpected ${name}`);
      }
    }),
  );
}

describe('opening any supported file', () => {
  afterEach(() => setKernelForTests(null));

  it('shows the job a vector file was sized to in a fresh Job Setup form', async () => {
    vectorKernel();
    // Job Setup is already showing, as it is at start up.
    useUiStore.getState().openForm('jobSetup');
    const serial = useUiStore.getState().formSerial;
    expect(await openVectorsAsJob('/art/plate.dxf')).toBe(true);
    expect(useDocumentStore.getState().job).toMatchObject({ width: 60, height: 30 });
    // The docks key the form on the serial: it must move on, or the form
    // keeps showing (and on OK applies) the old job's sizes.
    expect(useUiStore.getState().activeForm).toBe('jobSetup');
    expect(useUiStore.getState().formSerial).toBeGreaterThan(serial);
  });

  it('opens a second 3D model in a fresh import form', async () => {
    usePendingFileStore.getState().clear();
    await openFile('/models/first.stl');
    const serial = useUiStore.getState().formSerial;
    await openFile('/models/second.obj');
    expect(useUiStore.getState().activeForm).toBe('importModel');
    expect(useUiStore.getState().formSerial).toBeGreaterThan(serial);
    expect(pendingPath('mesh')).toBe('/models/second.obj');
  });

  it('closes a form left open when a job file opens or a new job starts', async () => {
    vectorKernel();
    useUiStore.getState().openForm('exportStl');
    await openFile('/jobs/sign.carve');
    expect(useDocumentStore.getState().job.width).toBe(500);
    expect(useUiStore.getState().activeForm).toBeNull();
    useUiStore.getState().openForm('exportStl');
    expect(await newJob()).toBe(true);
    expect(useUiStore.getState().activeForm).toBeNull();
  });

  it('quits at once when nothing is unsaved and asks first when something is', async () => {
    let asked = 0;
    let answer = false;
    let closed = 0;
    setKernelForTests({
      ...fakeKernel(() => {
        throw new Error('no kernel calls expected');
      }),
      confirm: () => {
        asked += 1;
        return Promise.resolve(answer);
      },
      allowClose: () => {
        closed += 1;
        return Promise.resolve();
      },
    });
    useDocumentStore.setState({ dirty: false });
    await closeRequested();
    expect([asked, closed]).toEqual([0, 1]);
    useDocumentStore.setState({ dirty: true });
    await closeRequested();
    expect([asked, closed]).toEqual([1, 1]);
    answer = true;
    await closeRequested();
    expect([asked, closed]).toEqual([2, 2]);
    useDocumentStore.setState({ dirty: false });
  });

  it('knows what each extension opens as', () => {
    expect(openKind('/jobs/Sign.carve')).toBe('job');
    expect(openKind('/art/Logo.SVG')).toBe('vectors');
    expect(openKind('/art/plan.dxf')).toBe('vectors');
    expect(openKind('/art/flyer.pdf')).toBe('vectors');
    expect(openKind('/models/bunny.stl')).toBe('mesh');
    expect(openKind('/models/ship.3ds')).toBe('mesh');
    expect(openKind('/photos/dog.jpeg')).toBe('bitmap');
    expect(openKind('/photos/dog.webp')).toBe('bitmap');
    expect(openKind('/clipart/Rose.carveclip')).toBe('clip');
    expect(openKind('/templates/Profiles.cstemplate')).toBe('toolpathTemplate');
  });

  it('refuses files it does not read', () => {
    expect(openKind('/notes/readme.txt')).toBeNull();
    expect(openKind('/folder.carve/noextension')).toBeNull();
    expect(openKind('C:\\jobs.svg\\file')).toBeNull();
    expect(openKind('')).toBeNull();
  });

  it('offers every supported file first in the open dialog', () => {
    const filters = openFilters();
    const all = filters[0];
    expect(all?.extensions).toContain('carve');
    expect(all?.extensions).toContain('stl');
    expect(all?.extensions).toContain('png');
    expect(all?.extensions).toContain('carveclip');
    const listed = filters.slice(1).flatMap((filter) => filter.extensions);
    expect([...listed].sort()).toEqual([...(all?.extensions ?? [])].sort());
  });

  it('sizes the job a vector file opens as to the vectors, where they were drawn', () => {
    const base = {
      ...DEFAULT_JOB,
      xyOrigin: 'centre' as const,
      offsetX: 5,
      kind: 'doubleSided' as const,
    };
    // Drawn up and right of the origin: the material runs from the origin.
    expect(jobForVectors(base, [20, 10, 420, 310])).toMatchObject({
      width: 420,
      height: 310,
      xyOrigin: 'bottomLeft',
      offsetX: 0,
      offsetY: 0,
      kind: 'singleSided',
      thickness: DEFAULT_JOB.thickness,
    });
    // Drawn round the origin: the datum sits inside the material.
    expect(jobForVectors(base, [-50, -20, 150, 80])).toMatchObject({
      width: 200,
      height: 100,
      offsetX: 50,
      offsetY: 20,
    });
    // One vertical line keeps the default width.
    expect(jobForVectors(base, [0, 0, 0, 90]).width).toBe(DEFAULT_JOB.width);
  });
});
