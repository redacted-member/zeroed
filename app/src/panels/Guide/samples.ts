// The finished jobs on the welcome card, and opening one as an untitled job.

import { api } from '../../bridge/formApi';
import { mayDiscard } from '../../lib/fileCommands';
import { dialogsStrings } from '../../lib/strings/dialogs';
import { useDocumentStore } from '../../state/documentStore';
import { useGuideStore } from '../../state/guideStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import type { GuideIconName } from '../GuideIcon/GuideIcon';

const text = dialogsStrings.guide.samples;

export interface Sample {
  id: string;
  title: string;
  blurb: string;
  icon: GuideIconName;
  /** A job best seen in 3D opens there, with Preview Toolpaths ready. */
  view: '2d' | '3d';
}

export const SAMPLES: readonly Sample[] = [
  { id: 'house-sign', ...text.houseSign, icon: 'sign', view: '2d' },
  { id: 'relief-plaque', ...text.reliefPlaque, icon: 'relief', view: '3d' },
  { id: 'box-joints', ...text.boxJoints, icon: 'box', view: '2d' },
  { id: 'knockdown-table', ...text.knockdownTable, icon: 'furniture', view: '2d' },
  { id: 'nested-sheet', ...text.nestedSheet, icon: 'nesting', view: '2d' },
  { id: 'two-sided', ...text.twoSided, icon: 'twoSided', view: '2d' },
  { id: 'rotary-sleeve', ...text.rotarySleeve, icon: 'rotary', view: '3d' },
  { id: 'laser-coaster', ...text.laserCoaster, icon: 'laser', view: '2d' },
];

/**
 * Opens a sample in place of the job, once any unsaved changes may go. True
 * when it opened; the welcome card and any open form close with it.
 */
export async function openSample(sample: Sample): Promise<boolean> {
  if (!(await mayDiscard())) {
    return false;
  }
  await api.samples.open(sample.id);
  const documents = useDocumentStore.getState();
  await documents.refresh();
  documents.jobReplaced();
  useViewStore.getState().zoomToJob(useDocumentStore.getState().job);
  const ui = useUiStore.getState();
  ui.closeForm();
  ui.setViewMode(sample.view);
  // A job best seen in 3D opens ready to watch it cut.
  if (sample.view === '3d') {
    ui.openForm('previewToolpaths');
  }
  useGuideStore.getState().close();
  return true;
}
