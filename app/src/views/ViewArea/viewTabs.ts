// The view bar's tabs: 2D View, 3D View and Assembly View. The Assembly
// View tab is the Assembly View form open over the 3D view; the other two
// show while it is closed. Pure, so the switching is tested alone.

import type { FormId, ViewMode } from '../../state/uiStore';

/** A tab of the view bar. */
export type ViewTab = '2d' | '3d' | 'assembly';

/** The form the Assembly View tab opens. */
export const ASSEMBLY_FORM: FormId = 'assemblyView';

/** The forms that show over the Assembly View: the view itself and the Furniture Designer. */
export function isAssemblyForm(form: FormId | null): boolean {
  return form === ASSEMBLY_FORM || form === 'furnitureDesigner';
}

/** Whether a tab shows as selected. */
export function tabPressed(tab: ViewTab, viewMode: ViewMode, activeForm: FormId | null): boolean {
  const assembly = isAssemblyForm(activeForm);
  if (tab === 'assembly') {
    return assembly;
  }
  return !assembly && (viewMode === tab || viewMode === 'split');
}

/** What a click on a tab does: the view to show, and whether the Assembly View form opens or closes. */
export function chooseTab(
  tab: ViewTab,
  activeForm: FormId | null,
): { viewMode: Exclude<ViewMode, 'split'>; form: 'open' | 'close' | 'keep' } {
  if (tab === 'assembly') {
    return { viewMode: '3d', form: isAssemblyForm(activeForm) ? 'keep' : 'open' };
  }
  return { viewMode: tab, form: isAssemblyForm(activeForm) ? 'close' : 'keep' };
}
