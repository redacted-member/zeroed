// Toolpaths > Create Job Sheet: the kernel writes an HTML summary of every
// toolpath (tools, depths, feeds, times and the material setup) to a file
// the user picks.

import { api } from '../bridge/api';
import { getKernel } from '../bridge/client';
import { useDocumentStore } from '../state/documentStore';
import { useToolpathStore } from '../state/toolpathStore';
import { fileName, withExtension } from './fileCommands';
import { strings } from './strings';

const text = strings.jobSheet;

/** The job's name without its extension, or the empty string for an unsaved job. */
export function jobTitle(jobPath: string | null): string {
  return jobPath === null ? '' : fileName(jobPath).replace(/\.[^.]*$/, '');
}

/** Default file name for the sheet. */
export function jobSheetName(jobPath: string | null): string {
  const title = jobTitle(jobPath);
  return `${title === '' ? text.suffix : `${title} ${text.suffix}`}.html`;
}

/** Asks where to write the job sheet and has the kernel write it. */
export async function createJobSheet(): Promise<void> {
  if (useToolpathStore.getState().toolpaths.length === 0) {
    useDocumentStore.setState({ lastError: text.noToolpaths });
    return;
  }
  const path = useDocumentStore.getState().path;
  const chosen = await getKernel().chooseSavePath(
    text.dialogTitle,
    [{ name: text.htmlFiles, extensions: ['html', 'htm'] }],
    jobSheetName(path),
  );
  if (chosen === null) {
    return;
  }
  try {
    await api.cam.jobSheet(withExtension(chosen, 'html', ['html', 'htm']), jobTitle(path));
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
  }
}
