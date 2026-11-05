// Cut List > Export CSV: builds the CSV in the job unit, asks where to
// write it with the save dialog every export uses, and has the kernel write
// it. A kernel without a text writer (io.saveText) gets the file handed to
// the browser as a download instead.

import type { PieceCutRow } from '../../bridge/assembly';
import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { KernelError } from '../../bridge/types';
import { fileName, withExtension } from '../../lib/fileCommands';
import { strings } from '../../lib/formStrings';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { cutListCsv, cutListName } from './model';

const text = strings.assembly;

/** Hands `csv` to the browser as a file download named `name`. */
function download(csv: string, name: string): void {
  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

/** Asks for a file and writes `rows` there. Answers the path written, or null. */
export async function exportCutList(rows: readonly PieceCutRow[]): Promise<string | null> {
  const doc = useDocumentStore.getState();
  const store = useAssemblyStore.getState();
  const csv = cutListCsv(rows, doc.job.units);
  const chosen = await getKernel().chooseSavePath(
    text.exportTitle,
    [{ name: text.csvFiles, extensions: ['csv'] }],
    cutListName(doc.path),
  );
  if (chosen === null) {
    return null;
  }
  const path = withExtension(chosen, 'csv');
  try {
    await api.io.saveText(path, csv);
    store.setNotice(text.exported(path));
  } catch (error) {
    if (!(error instanceof KernelError) || error.code !== 'unknown_command') {
      store.setNotice(null);
      store.setError(error instanceof Error ? error.message : String(error));
      return null;
    }
    download(csv, fileName(path));
    store.setNotice(text.downloaded(fileName(path)));
  }
  return path;
}
