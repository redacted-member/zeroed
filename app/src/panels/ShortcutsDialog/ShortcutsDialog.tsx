import { useMemo, type JSX } from 'react';

import { dialogsStrings } from '../../lib/strings/dialogs';
import { useDialogStore } from '../../state/dialogStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { shortcutSections } from './model';

import './ShortcutsDialog.scss';

const text = dialogsStrings.shortcutsDialog;
const MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);

/** Help > Keyboard Shortcuts: every key the app answers. */
export function ShortcutsDialog(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'shortcuts');
  const close = useDialogStore((state) => state.close);
  return (
    <ModalDialog
      className="shortcuts-dialog"
      closeLabel={text.close}
      onClose={close}
      open={open}
      title={text.title}
      titleId="shortcuts-dialog-title"
    >
      <ShortcutsBody />
    </ModalDialog>
  );
}

function ShortcutsBody(): JSX.Element {
  const sections = useMemo(() => shortcutSections(MAC), []);
  return (
    <div className="shortcuts-dialog__columns">
      {sections.map((section) => (
        <table className="shortcuts-dialog__table" key={section.id}>
          <caption className="shortcuts-dialog__caption">{section.title}</caption>
          <thead className="shortcuts-dialog__head">
            <tr>
              <th scope="col">{text.key}</th>
              <th scope="col">{text.action}</th>
            </tr>
          </thead>
          <tbody>
            {section.rows.map((row) => (
              <tr className="shortcuts-dialog__row" key={row.id}>
                <td className="shortcuts-dialog__keys">
                  {row.keys.map((key, index) => (
                    <span key={key}>
                      {index > 0 ? <span className="shortcuts-dialog__or"> {text.or} </span> : null}
                      <kbd className="shortcuts-dialog__kbd">{key}</kbd>
                    </span>
                  ))}
                </td>
                <td className="shortcuts-dialog__label">{row.label}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ))}
    </div>
  );
}
