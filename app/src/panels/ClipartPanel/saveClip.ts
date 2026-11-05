// Save as Clipart: writes one component to a clip file, offered from the
// Components panel's right click menu.

import { api } from '../../bridge/api';
import { getKernel } from '../../bridge/client';
import { withExtension } from '../../lib/fileCommands';
import { strings } from '../../lib/strings';
import { useClipartStore } from './store';

/** Extension of a clip file, without the dot. */
export const CLIP_EXTENSION = 'carveclip';

/** Default file name for a component called `name`. */
export function clipFileName(name: string): string {
  const base = name.trim().replace(/[\\/:*?"<>|]+/g, '_');
  return `${base === '' ? strings.clipart.untitled : base}.${CLIP_EXTENSION}`;
}

/** Asks where to save, then writes the component and rescans the library. */
export async function saveComponentAsClip(id: number, name: string): Promise<void> {
  const store = useClipartStore.getState();
  const userDir = store.userDir;
  const suggested = clipFileName(name);
  const chosen = await getKernel().chooseSavePath(
    strings.clipart.saveTitle,
    [{ name: strings.clipart.files, extensions: [CLIP_EXTENSION] }],
    userDir === null ? suggested : `${userDir}/${suggested}`,
  );
  if (chosen === null) {
    return;
  }
  try {
    const reply = await api.clipart.save(id, withExtension(chosen, CLIP_EXTENSION), name);
    store.setStatus(strings.clipart.saved(reply.path));
    await store.scan();
  } catch (error) {
    useClipartStore.setState({ error: error instanceof Error ? error.message : String(error) });
  }
}
