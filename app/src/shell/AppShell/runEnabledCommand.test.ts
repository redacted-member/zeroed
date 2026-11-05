import { afterEach, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../../bridge/client';
import { useDocumentStore } from '../../state/documentStore';
import { fakeKernel } from '../../testing/fakeKernel';
import { runEnabledCommand } from './AppShell';

describe('runEnabledCommand', () => {
  afterEach(() => setKernelForTests(null));

  it('asks the kernel again before dropping a greyed Undo', async () => {
    const calls: string[] = [];
    setKernelForTests(
      fakeKernel((name) => {
        calls.push(name);
        if (name === 'undo.info') {
          return { canUndo: true, canRedo: false, undoLabel: 'Move', redoLabel: null };
        }
        if (name === 'undo') {
          return { revision: 2, label: 'Move' };
        }
        throw new Error(`unexpected ${name}`);
      }),
    );
    useDocumentStore.setState({
      undo: { canUndo: false, canRedo: false, undoLabel: null, redoLabel: null },
    });
    await runEnabledCommand('edit.undo');
    expect(calls.slice(0, 2)).toEqual(['undo.info', 'undo']);
  });

  it('drops Redo when the kernel still has nothing to redo', async () => {
    const calls: string[] = [];
    setKernelForTests(
      fakeKernel((name) => {
        calls.push(name);
        return { canUndo: false, canRedo: false, undoLabel: null, redoLabel: null };
      }),
    );
    await runEnabledCommand('edit.redo');
    expect(calls).toEqual(['undo.info']);
  });
});
