import { Suspense, useEffect, useState, type JSX } from 'react';

import { lazyNamed } from '../../lib/lazyNamed';
import { useDialogStore, type DialogId } from '../../state/dialogStore';
import { useGuideStore } from '../../state/guideStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useTemplateStore } from '../../state/templateStore';
import { useUiStore } from '../../state/uiStore';

// The dialogs load the first time they open, then stay mounted exactly as
// they were before, so their state between openings does not change.
const ToolDatabase = lazyNamed(
  () => import('../../panels/ToolDatabase/ToolDatabase'),
  'ToolDatabase',
);
const HelpDialog = lazyNamed(() => import('../../panels/HelpDialog/HelpDialog'), 'HelpDialog');
const MachineConfiguration = lazyNamed(
  () => import('../../panels/MachineConfiguration/MachineConfiguration'),
  'MachineConfiguration',
);
const PostManagement = lazyNamed(
  () => import('../../panels/PostManagement/PostManagement'),
  'PostManagement',
);
const Preferences = lazyNamed(() => import('../../panels/Preferences/Preferences'), 'Preferences');
const ShortcutsDialog = lazyNamed(
  () => import('../../panels/ShortcutsDialog/ShortcutsDialog'),
  'ShortcutsDialog',
);
const MaterialManagement = lazyNamed(
  () => import('../../panels/MaterialManagement/MaterialManagement'),
  'MaterialManagement',
);
const NotesDialog = lazyNamed(() => import('../../panels/NotesDialog/NotesDialog'), 'NotesDialog');
const InfoDialog = lazyNamed(() => import('../../panels/InfoDialog/InfoDialog'), 'InfoDialog');
const MachiningTimeDialog = lazyNamed(
  () => import('../../panels/MachiningTimeDialog/MachiningTimeDialog'),
  'MachiningTimeDialog',
);
const MissingLayersDialog = lazyNamed(
  () => import('../../panels/MissingLayersDialog/MissingLayersDialog'),
  'MissingLayersDialog',
);
const PdfPageDialog = lazyNamed(
  () => import('../../panels/PdfPageDialog/PdfPageDialog'),
  'PdfPageDialog',
);
const MenuVectorSelector = lazyNamed(
  () => import('../../panels/VectorSelector/MenuVectorSelector'),
  'MenuVectorSelector',
);
const ToolpathWarningsDialog = lazyNamed(
  () => import('../../panels/ToolpathWarningsDialog/ToolpathWarningsDialog'),
  'ToolpathWarningsDialog',
);
const TemplateDialog = lazyNamed(
  () => import('../../panels/TemplateDialog/TemplateDialog'),
  'TemplateDialog',
);

const FurnitureLibrary = lazyNamed(
  () => import('../../panels/FurnitureLibrary/FurnitureLibrary'),
  'FurnitureLibrary',
);

const Guide = lazyNamed(() => import('../../panels/Guide/Guide'), 'Guide');

const INFO: readonly DialogId[] = ['whatsNew', 'releaseNotes', 'licences', 'about'];

/** True from the first render where `open` is true. */
function useOpenedOnce(open: boolean): boolean {
  const [opened, setOpened] = useState(open);
  if (open && !opened) {
    setOpened(true);
  }
  return opened || open;
}

/** Whether this session has offered the getting started guide yet. */
let guideOffered = false;

/** Opens the getting started guide once the preferences say it shows at start. */
function useFirstRunGuide(): void {
  const showAtStart = usePrefsStore((state) => state.loaded && state.prefs.general.showGuide);
  useEffect(() => {
    if (showAtStart && !guideOffered) {
      guideOffered = true;
      useGuideStore.getState().showWelcome();
    }
  }, [showAtStart]);
}

/** The menus' dialogs, each loaded on demand the first time it opens. */
export function LazyPanels(): JSX.Element {
  const toolDatabase = useOpenedOnce(useUiStore((ui) => ui.toolDatabaseOpen));
  const preferences = useOpenedOnce(usePrefsStore((state) => state.dialogOpen));
  const templates = useOpenedOnce(useTemplateStore((state) => state.dialog !== null));
  const dialog = useDialogStore((state) => state.open);
  const help = useOpenedOnce(dialog === 'helpContents');
  const machine = useOpenedOnce(dialog === 'machineConfiguration');
  const posts = useOpenedOnce(dialog === 'postManagement');
  const shortcuts = useOpenedOnce(dialog === 'shortcuts');
  const materials = useOpenedOnce(dialog === 'materials');
  const notes = useOpenedOnce(dialog === 'notes');
  const info = useOpenedOnce(dialog !== null && INFO.includes(dialog));
  const pdfPage = useOpenedOnce(dialog === 'pdfPage');
  const missingLayers = useOpenedOnce(dialog === 'missingLayers');
  const machiningTime = useOpenedOnce(dialog === 'machiningTime');
  const vectorSelector = useOpenedOnce(dialog === 'vectorSelector');
  const toolpathWarnings = useOpenedOnce(dialog === 'toolpathWarnings');
  const furnitureLibrary = useOpenedOnce(dialog === 'furnitureLibrary');
  const guide = useOpenedOnce(useGuideStore((state) => state.open));
  useFirstRunGuide();

  return (
    <Suspense fallback={null}>
      {toolDatabase ? <ToolDatabase /> : null}
      {help ? <HelpDialog /> : null}
      {machine ? <MachineConfiguration /> : null}
      {posts ? <PostManagement /> : null}
      {preferences ? <Preferences /> : null}
      {templates ? <TemplateDialog /> : null}
      {shortcuts ? <ShortcutsDialog /> : null}
      {materials ? <MaterialManagement /> : null}
      {notes ? <NotesDialog /> : null}
      {info ? <InfoDialog /> : null}
      {pdfPage ? <PdfPageDialog /> : null}
      {missingLayers ? <MissingLayersDialog /> : null}
      {machiningTime ? <MachiningTimeDialog /> : null}
      {vectorSelector ? <MenuVectorSelector /> : null}
      {toolpathWarnings ? <ToolpathWarningsDialog /> : null}
      {furnitureLibrary ? <FurnitureLibrary /> : null}
      {guide ? <Guide /> : null}
    </Suspense>
  );
}
