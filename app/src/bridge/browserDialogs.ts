// File and question dialogs for the browser transports (mock and dev host),
// which have no native dialogs. The page asks for a path as text; the dev
// host then reads or writes it on this machine.

import type { FileFilter } from './types';

function ask(title: string, filters: FileFilter[], defaultPath?: string): Promise<string | null> {
  const kinds = filters.map((f) => f.extensions.map((e) => `.${e}`).join(' ')).join(', ');
  const answer = window.prompt(`${title} (${kinds})`, defaultPath ?? '');
  return Promise.resolve(answer === null || answer.trim() === '' ? null : answer.trim());
}

export const browserDialogs = {
  chooseOpenPath: (title: string, filters: FileFilter[], defaultPath?: string) =>
    ask(title, filters, defaultPath),
  chooseSavePath: (title: string, filters: FileFilter[], defaultPath?: string) =>
    ask(title, filters, defaultPath),
  confirm: (message: string) => Promise.resolve(window.confirm(message)),
};
