import { afterEach, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import { pendingPath, usePendingFileStore } from '../state/pendingFileStore';
import { useUiStore } from '../state/uiStore';
import { fakeKernel } from '../testing/fakeKernel';
import { importModelCommand } from './importModel';

function kernelChoosing(path: string | null): void {
  setKernelForTests({
    ...fakeKernel(() => {
      throw new Error('no kernel calls expected');
    }),
    chooseOpenPath: () => Promise.resolve(path),
  });
}

describe('Import Component', () => {
  afterEach(() => {
    setKernelForTests(null);
    usePendingFileStore.getState().clear();
    useUiStore.getState().closeForm();
  });

  it('opens the import form, which asks for the file itself', async () => {
    kernelChoosing('/models/never-asked.stl');
    useUiStore.getState().closeForm();
    await importModelCommand();
    expect(useUiStore.getState().activeForm).toBe('importModel');
    expect(pendingPath('mesh')).toBeNull();
  });

  it('loads a new file into a fresh form while the import shows', async () => {
    kernelChoosing('/models/second.obj');
    useUiStore.getState().openForm('importModel');
    const serial = useUiStore.getState().formSerial;
    await importModelCommand();
    expect(useUiStore.getState().activeForm).toBe('importModel');
    expect(useUiStore.getState().formSerial).toBeGreaterThan(serial);
    expect(pendingPath('mesh')).toBe('/models/second.obj');
  });

  it('leaves the import as it was when no file is chosen', async () => {
    kernelChoosing(null);
    useUiStore.getState().openForm('positionModel');
    const serial = useUiStore.getState().formSerial;
    await importModelCommand();
    expect(useUiStore.getState().activeForm).toBe('positionModel');
    expect(useUiStore.getState().formSerial).toBe(serial);
    expect(pendingPath('mesh')).toBeNull();
  });
});
