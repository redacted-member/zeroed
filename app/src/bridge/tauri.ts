// Real kernel client. Talks to the shell over the Tauri command bridge;
// frame.ts decodes the replies.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { MENU_COMMAND_EVENT, isMenuCommand, type MenuCommand } from '../lib/menu';
import { blobResult, jsonResult, splitFrame, type Framed, type RawReply } from './frame';
import {
  KernelError,
  type BlobHeader,
  type BlobReply,
  type ChoiceButtons,
  type FileFilter,
  type KernelActivity,
  type KernelClient,
  type ProgressEvent,
  type SessionRecovery,
} from './types';

const PROGRESS_EVENT = 'kernel://progress';
const OPEN_RECENT_EVENT = 'menu:open-recent';
const GADGET_EVENT = 'menu:gadget';
/** Emitted by the shell when the system hands it files to open. */
const OPEN_FILES_EVENT = 'app://open-files';
/** Emitted by the shell when the window's close button or Quit is used. */
const CLOSE_REQUESTED_EVENT = 'app://close-requested';

// The window, webview and dialog modules load on first use, outside the start chunk.
const dialogs = () => import('@tauri-apps/plugin-dialog');

async function invokeRaw(name: string, args: unknown): Promise<Framed> {
  let raw: RawReply;
  try {
    raw = await invoke<RawReply>('kernel_call', {
      name,
      args: args === undefined ? '' : JSON.stringify(args),
    });
  } catch (error) {
    throw new KernelError(name, String(error), 'transport');
  }
  return splitFrame(name, raw);
}

/** Runs one kernel command and unwraps its JSON reply envelope. */
export async function call<T>(name: string, args?: unknown): Promise<T> {
  return jsonResult<T>(name, await invokeRaw(name, args));
}

/** Runs one kernel command that answers with a binary reply. */
export async function callBlob<H extends BlobHeader>(
  name: string,
  args?: unknown,
): Promise<BlobReply<H>> {
  return blobResult<H>(name, await invokeRaw(name, args));
}

/** Creates the client used when the front end runs inside the shell. */
export function createTauriKernel(): KernelClient {
  return {
    kind: 'tauri',
    call,
    callBlob,
    cancel: async (id: number) => {
      try {
        return await invoke<boolean>('kernel_cancel', { id });
      } catch (error) {
        throw new KernelError('kernel_cancel', String(error), 'transport');
      }
    },
    activity: () => invoke<KernelActivity>('kernel_progress'),
    onMenuCommand: async (handler: (command: MenuCommand) => void) => {
      const unlisten = await listen<string>(MENU_COMMAND_EVENT, (event) => {
        if (isMenuCommand(event.payload)) {
          handler(event.payload);
        }
      });
      return () => unlisten();
    },
    onProgress: async (handler: (event: ProgressEvent) => void) => {
      const unlisten = await listen<ProgressEvent>(PROGRESS_EVENT, (event) => {
        handler(event.payload);
      });
      return () => unlisten();
    },
    chooseOpenPath: async (title: string, filters: FileFilter[], defaultPath?: string) => {
      const options = { title, filters, multiple: false, directory: false } as const;
      const { open } = await dialogs();
      const chosen = await open(defaultPath === undefined ? options : { ...options, defaultPath });
      return typeof chosen === 'string' ? chosen : null;
    },
    chooseSavePath: async (title: string, filters: FileFilter[], defaultPath?: string) => {
      const { save } = await dialogs();
      return save(defaultPath === undefined ? { title, filters } : { title, filters, defaultPath });
    },
    confirm: async (message: string) => {
      const { confirm } = await dialogs();
      return confirm(message, { kind: 'warning' });
    },
    setWindowTitle: async (title: string) => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        await getCurrentWindow().setTitle(title);
      } catch (error) {
        console.error('the window title could not be set', error);
      }
    },
    setMenus: async (menus: unknown[]) => {
      try {
        await invoke('menu_set', { menus });
      } catch (error) {
        console.error('the menu bar could not be updated', error);
      }
    },
    setMenuIcons: async (images: Record<string, number[]>) => {
      try {
        await invoke('menu_icons', { images });
      } catch (error) {
        console.error('the menu icons could not be set', error);
      }
    },
    openDataFolder: async (sub?: 'posts' | 'furniture') => {
      try {
        await invoke('open_data_folder', { sub: sub ?? null });
      } catch (error) {
        console.error('the application data folder could not be opened', error);
      }
    },
    editPost: async (id: string) => {
      await invoke('edit_post', { id });
    },
    directOutputPath: (name: string) => invoke<string>('direct_output_path', { name }),
    sendToDriver: async (paths: string[], driver: string) => {
      await invoke('send_to_driver', { paths, driver });
    },
    onMenuGadget: async (handler: (name: string) => void) => {
      const unlisten = await listen<string>(GADGET_EVENT, (event) => {
        handler(event.payload);
      });
      return () => unlisten();
    },
    onOpenRecent: async (handler: (index: number) => void) => {
      const unlisten = await listen<number>(OPEN_RECENT_EVENT, (event) => {
        handler(event.payload);
      });
      return () => unlisten();
    },
    onOpenFiles: async (handler: (paths: string[]) => void) => {
      // Files opened before the page listened wait in the shell's queue.
      const drain = async () => {
        try {
          const paths = await invoke<string[]>('take_opened_files');
          if (paths.length > 0) {
            handler(paths);
          }
        } catch (error) {
          console.error('the files to open could not be read', error);
        }
      };
      const stopOpened = await listen(OPEN_FILES_EVENT, () => void drain());
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      const stopDrop = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === 'drop' && event.payload.paths.length > 0) {
          handler(event.payload.paths);
        }
      });
      await drain();
      return () => {
        stopOpened();
        stopDrop();
      };
    },
    onCloseRequested: async (handler: () => void) => {
      const stop = await listen(CLOSE_REQUESTED_EVENT, () => handler());
      await invoke('close_guard', { armed: true });
      return () => {
        stop();
        void invoke('close_guard', { armed: false });
      };
    },
    allowClose: () => invoke<void>('close_approved'),
    choose: async (message: string, buttons: ChoiceButtons) => {
      const { message: ask } = await dialogs();
      const pressed = await ask(message, { kind: 'warning', buttons });
      if (pressed === buttons.yes || pressed === 'Yes') return 'yes';
      if (pressed === buttons.no || pressed === 'No') return 'no';
      return 'cancel';
    },
    setDocumentState: async (edited: boolean, path: string | null) => {
      try {
        await invoke('set_document_state', { edited, path });
      } catch (error) {
        console.error('the window document state could not be set', error);
      }
    },
    sessionRecovery: () => invoke<SessionRecovery>('session_recovery'),
    recordBackup: async (path: string | null) => {
      try {
        await invoke('session_backup', { path });
      } catch (error) {
        console.error('the backup could not be recorded', error);
      }
    },
    discardBackup: (path: string) => invoke<void>('discard_backup', { path }),
    keepBackup: (path: string) => invoke<string>('keep_backup', { path }),
    showCrashReport: (path: string) => invoke<void>('show_crash_report', { path }),
  };
}
