// Shared by the three model import forms: marks which step is open so the
// 3D view previews it, and drops the flow once the panel moves on to a form
// that is not one of the steps.

import { useEffect } from 'react';

import { useImportStore, type ImportStep } from '../state/importStore';
import { useUiStore, type FormId } from '../state/uiStore';

export const IMPORT_FORMS: ReadonlySet<FormId> = new Set<FormId>([
  'importModel',
  'positionModel',
  'segmentModel',
]);

/** Whether the flow ends when the open form becomes `form`. */
export function leavesImport(form: FormId | null): boolean {
  return form === null || !IMPORT_FORMS.has(form);
}

export function useImportStep(step: ImportStep): void {
  useEffect(() => {
    useImportStore.getState().setView({ step });
    void useImportStore.getState().refresh();
    return () => {
      // The next form opens in the same update; look once it has.
      queueMicrotask(() => {
        if (leavesImport(useUiStore.getState().activeForm)) {
          useImportStore.getState().clear();
        }
      });
    };
  }, [step]);
}
