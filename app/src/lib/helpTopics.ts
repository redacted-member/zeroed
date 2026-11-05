// Ids of the help topics the shell opens: a form's (its help button, F1), a
// dock page's (the dock title's help) and the getting started pages. The
// help content itself loads with the Help dialog (panels/HelpDialog).

import { formDock } from '../state/formDock';
import type { FormId, WorkspaceTab } from '../state/uiStore';

/** The page the help opens on when nothing more specific applies. */
export const START_TOPIC = 'page:start';
/** The checklist to work through before the first real cut. */
export const BEFORE_YOU_CUT_TOPIC = 'page:beforeYouCut';
/** The Toolpaths panel's page. */
export const TOOLPATHS_TOPIC = 'panel:toolpaths';

/** The topic of a form. */
export function formTopic(form: FormId): string {
  return `form:${form}`;
}

/** The topic of a left dock tab. */
export function tabTopic(tab: WorkspaceTab): string {
  return `panel:${tab}`;
}

/** The topic a dock title's help opens: the form open in that dock, else the dock's page. */
export function dockTopic(
  side: 'left' | 'right',
  activeForm: FormId | null,
  activeTab: WorkspaceTab,
): string {
  if (activeForm !== null && formDock(activeForm) === side) {
    return formTopic(activeForm);
  }
  return side === 'left' ? tabTopic(activeTab) : TOOLPATHS_TOPIC;
}

/** The topic Help > CarveStudio Help and F1 open: the open form's, else the start page. */
export function contextTopic(activeForm: FormId | null): string {
  return activeForm === null ? START_TOPIC : formTopic(activeForm);
}
