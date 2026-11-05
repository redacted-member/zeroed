import type { JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { warningCount } from '../../lib/toolpathWarnings';
import { useDialogStore } from '../../state/dialogStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';

import './ToolpathWarningsDialog.scss';

const text = strings.toolpaths.warnings;

/** What the last toolpath calculation warned about, shown once it is done. */
export function ToolpathWarningsDialog(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'toolpathWarnings');
  const close = useDialogStore((state) => state.close);
  const warnings = useToolpathStore((state) => state.warnings);

  return (
    <ModalDialog
      className="toolpath-warnings"
      closeLabel={text.ok}
      onClose={close}
      open={open && warnings.length > 0}
      title={text.title}
      titleId="toolpath-warnings-title"
    >
      <div className="toolpath-warnings__body">
        <p className="toolpath-warnings__intro">{text.intro(warningCount(warnings))}</p>
        <ul className="toolpath-warnings__list">
          {warnings.map((warning, index) => (
            <li className="toolpath-warnings__item" key={`${index}-${warning.toolpath ?? ''}`}>
              <h3 className="toolpath-warnings__toolpath">{warning.toolpath ?? text.general}</h3>
              <ul className="toolpath-warnings__messages">
                {warning.messages.map((message) => (
                  <li className="toolpath-warnings__message" key={message}>
                    {message}
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      </div>
    </ModalDialog>
  );
}
