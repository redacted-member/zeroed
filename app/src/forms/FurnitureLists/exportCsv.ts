// Export CSV for the designer's lists: asks where to write it with the
// save dialog every export uses and has the kernel write it; a kernel
// without a text writer (io.saveText) hands the file to the browser.

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { KernelError } from '../../bridge/types';
import { fileName, withExtension } from '../../lib/fileCommands';
import { furnitureStrings } from '../../lib/strings/furniture';

const text = furnitureStrings.furniture.designer;

function download(csv: string, name: string): void {
  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

/** Writes `csv` where the user says; answers the notice to show, or null when cancelled. */
export async function exportCsv(csv: string, suggested: string): Promise<string | null> {
  const chosen = await getKernel().chooseSavePath(
    text.exportTitle,
    [{ name: text.csvFiles, extensions: ['csv'] }],
    suggested,
  );
  if (chosen === null) {
    return null;
  }
  const path = withExtension(chosen, 'csv');
  try {
    await api.io.saveText(path, csv);
    return text.exported(path);
  } catch (error) {
    if (!(error instanceof KernelError) || error.code !== 'unknown_command') {
      throw error;
    }
    download(csv, fileName(path));
    return text.downloaded(fileName(path));
  }
}
