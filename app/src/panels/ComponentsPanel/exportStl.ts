// Export Model: Save Triangulation asks for a file and writes the chosen
// component, or the whole model when none is chosen, as a mesh in the
// format of the file type picked (STL, OBJ or POV-Ray); Export as Grayscale
// Bitmap writes the model as a 16 bit greyscale PNG.

import { getKernel } from '../../bridge/client';
import type { MeshOptions } from '../../bridge/exports';
import type { FileFilter } from '../../bridge/types';
import {
  MESH_EXPORT_FORMATS,
  meshFormatOf,
  type MeshExportFormat,
} from '../../forms/ExportStlForm/model';
import { withExtension } from '../../lib/fileCommands';
import { strings } from '../../lib/formStrings';
import { useModelStore } from '../../state/modelStore';

const text = strings.model.exportStl;

/** Components the export covers: the selected one, or undefined for the composite. */
export function stlExportIds(selectedId: number | null): number[] | undefined {
  return selectedId === null ? undefined : [selectedId];
}

/** Default file name offered in the save dialog. */
export function stlDefaultName(name: string | null, extension = 'stl'): string {
  const base = (name ?? '').trim().replace(/[\\/:*?"<>|]+/g, '_');
  return `${base === '' ? strings.file.untitled : base}.${extension}`;
}

/** The save dialog's file types for a mesh, STL first. */
export function meshFilters(): FileFilter[] {
  return MESH_EXPORT_FORMATS.map((format: MeshExportFormat) => ({
    name: text.formatFiles[format],
    extensions: [format],
  }));
}

function selectedName(): string | null {
  const { selectedId, components } = useModelStore.getState();
  return components.find((c) => c.id === selectedId)?.name ?? null;
}

/** Save Triangulation: asks where to write, then meshes and closes the
 * model as `options` say, in the format of the file type chosen. Answers
 * success. */
export async function saveTriangulation(options: MeshOptions = {}): Promise<boolean> {
  const chosen = await getKernel().chooseSavePath(
    text.dialogTitle,
    meshFilters(),
    stlDefaultName(selectedName()),
  );
  if (chosen === null) {
    return false;
  }
  const format = meshFormatOf(chosen);
  const path = withExtension(chosen, format);
  const store = useModelStore.getState();
  return store.exportMesh(path, format, stlExportIds(store.selectedId), options);
}

/** Export as Grayscale Bitmap: the model (or the selected component) as a
 * 16 bit greyscale PNG, white highest. Answers success. */
export async function exportGrayscale(): Promise<boolean> {
  const chosen = await getKernel().chooseSavePath(
    text.grayscaleTitle,
    [{ name: text.grayscaleFiles, extensions: ['png'] }],
    stlDefaultName(selectedName(), 'png'),
  );
  if (chosen === null) {
    return false;
  }
  const store = useModelStore.getState();
  return store.exportHeightmap(withExtension(chosen, 'png'), 16, stlExportIds(store.selectedId));
}
