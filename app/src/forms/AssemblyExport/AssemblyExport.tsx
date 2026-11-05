import { useId, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import { withExtension } from '../../lib/fileCommands';
import { assemblyFitStrings as text } from '../../lib/strings/assemblyFit';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { exportName, type MeshFormat } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { whyStrings } from '../../lib/strings/why';

import './AssemblyExport.scss';

const FORMATS: readonly MeshFormat[] = ['stl', '3mf'];

/** Export Assembly: the placed pieces, as assembled, in one STL or 3MF file. */
export function AssemblyExport(): JSX.Element {
  const pieces = useAssemblyStore((s) => s.pieces);
  const [format, setFormat] = useState<MeshFormat>('stl');
  const [busy, setBusy] = useState(false);
  const formatId = useId();

  const run = async () => {
    const store = useAssemblyStore.getState();
    const chosen = await getKernel().chooseSavePath(
      text.exportTitle,
      [{ name: text.formatName[format], extensions: [format] }],
      exportName(useDocumentStore.getState().path, format),
    );
    if (chosen === null) {
      return;
    }
    const path = withExtension(chosen, format);
    setBusy(true);
    try {
      const reply = await api.assembly.exportAssembly(path, format);
      store.setError(null);
      store.setNotice(text.exported(reply.pieces, path));
    } catch (error) {
      store.setNotice(null);
      store.setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const exportWhy = firstWhy(
    [busy, whyStrings.why.busy],
    [pieces.length === 0, whyStrings.why.noPieces],
  );
  return (
    <div className="assembly-export">
      <label className="assembly-export__label" htmlFor={formatId}>
        {text.format}
      </label>
      <select
        className="assembly-export__select"
        id={formatId}
        onChange={(event) => setFormat(event.target.value === '3mf' ? '3mf' : 'stl')}
        value={format}
      >
        {FORMATS.map((f) => (
          <option key={f} value={f}>
            {text.formatName[f]}
          </option>
        ))}
      </select>
      <button
        className="assembly-export__button"
        disabled={exportWhy !== undefined}
        onClick={() => void run()}
        type="button"
        data-tip={whyTip(text.exportAssembly, exportWhy)}
      >
        <ButtonFace icon="exportModel" label={text.exportAssembly} />
      </button>
    </div>
  );
}
