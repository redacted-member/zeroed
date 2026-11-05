// Export Assembly model: the file formats and the default file name.

/** The mesh files Export Assembly writes. */
export type MeshFormat = 'stl' | '3mf';

/** The default export file name for a job path: the job's name plus Assembly. */
export function exportName(jobPath: string | null, format: MeshFormat): string {
  const base =
    jobPath
      ?.split(/[/\\]/)
      .pop()
      ?.replace(/\.[^.]*$/, '') ?? '';
  return `${base === '' ? 'Assembly' : `${base} Assembly`}.${format}`;
}
