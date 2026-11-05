import type { JSX } from 'react';

import { strings } from '../../lib/strings';
import { dialogsStrings } from '../../lib/strings/dialogs';
import { useDialogStore, type DialogId } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';

import './InfoDialog.scss';

const text = dialogsStrings.infoDialogs;

type InfoId = Extract<DialogId, 'whatsNew' | 'releaseNotes' | 'licences' | 'about'>;

const TITLES: Readonly<Record<InfoId, string>> = {
  whatsNew: text.whatsNewTitle,
  releaseNotes: text.releaseNotesTitle,
  licences: text.licencesTitle,
  about: text.aboutTitle,
};

function isInfo(id: DialogId | null): id is InfoId {
  return id === 'whatsNew' || id === 'releaseNotes' || id === 'licences' || id === 'about';
}

/** Help > What's New, Release Notes, Third Party Licences and About. */
export function InfoDialog(): JSX.Element {
  const open = useDialogStore((state) => state.open);
  const close = useDialogStore((state) => state.close);
  const shown = isInfo(open) ? open : null;
  return (
    <ModalDialog
      className="info-dialog"
      closeLabel={text.close}
      onClose={close}
      open={shown !== null}
      title={shown === null ? '' : TITLES[shown]}
      titleId="info-dialog-title"
    >
      {shown === null ? null : <InfoBody id={shown} />}
    </ModalDialog>
  );
}

function InfoBody({ id }: { id: InfoId }): JSX.Element {
  const version = useDocumentStore((state) => state.kernelVersion);
  switch (id) {
    case 'whatsNew':
    case 'releaseNotes':
      return (
        <ul className="info-dialog__list">
          {(id === 'whatsNew' ? text.whatsNew : text.releaseNotes).map((line) => (
            <li className="info-dialog__item" key={line}>
              {line}
            </li>
          ))}
        </ul>
      );
    case 'licences':
      return (
        <div className="info-dialog__body">
          <p className="info-dialog__text">{text.licencesIntro}</p>
          <table className="info-dialog__table">
            <thead>
              <tr>
                <th className="info-dialog__head" scope="col">
                  {text.project}
                </th>
                <th className="info-dialog__head" scope="col">
                  {text.licence}
                </th>
              </tr>
            </thead>
            <tbody>
              {text.licences.map(([project, licence]) => (
                <tr className="info-dialog__row" key={project}>
                  <td className="info-dialog__cell">{project}</td>
                  <td className="info-dialog__cell">{licence}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case 'about':
      return (
        <div className="info-dialog__body">
          <p className="info-dialog__name">{strings.app.name}</p>
          <p className="info-dialog__text">{text.about}</p>
          <p className="info-dialog__text">
            {version === null ? text.unknownVersion : text.version(version)}
          </p>
        </div>
      );
  }
}
