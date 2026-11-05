import { useId, useState, type JSX } from 'react';

import { pieceKey, type MateKind, type MateRow, type PieceRef } from '../../bridge/assembly';
import { strings } from '../../lib/formStrings';
import { assemblyMatesStrings } from '../../lib/strings/assemblyMates';
import { unitSuffix } from '../../lib/units';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { pieceLabel } from '../AssemblyViewForm/model';
import { NumberField } from '../NumberField/NumberField';
import {
  conflictCount,
  kindsFor,
  mateRowText,
  mateValueField,
  mateValueText,
  offText,
  parseMateValue,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './AssemblyMates.scss';

const text = assemblyMatesStrings.mates;

/**
 * The mates kept with the job, under the pieces: each with its kind, its
 * two pieces and how it stands (a conflict says how far off it is). A
 * picked mate can be changed (its kind, value and, for a distance, which
 * way the faces look), suppressed or deleted; each is one undo step and
 * solves the mates again.
 */
export function AssemblyMates(): JSX.Element {
  const mates = useAssemblyStore((s) => s.mates);
  const pieces = useAssemblyStore((s) => s.pieces);
  const selectedMate = useAssemblyStore((s) => s.selectedMate);
  const selectMate = useAssemblyStore((s) => s.selectMate);
  const units = useDocumentStore((s) => s.job.units);
  const nameOf = (ref: PieceRef) => {
    const row = pieces.find((p) => pieceKey(p) === pieceKey(ref));
    return row === undefined ? pieceKey(ref) : pieceLabel(row);
  };
  const picked = mates.find((m) => m.id === selectedMate) ?? null;
  const conflicts = conflictCount(mates);

  return (
    <section aria-labelledby="assembly-mates-heading" className="assembly-mates">
      <h3 className="assembly-mates__heading" id="assembly-mates-heading">
        {text.title}
      </h3>
      {mates.length === 0 ? (
        <p className="assembly-mates__empty">{text.empty}</p>
      ) : (
        <ul aria-labelledby="assembly-mates-heading" className="assembly-mates__list">
          {mates.map((row) => {
            const line = mateRowText(row, nameOf);
            const flagged = row.state !== 'solved';
            return (
              <li key={row.id}>
                <button
                  aria-pressed={row.id === selectedMate}
                  className={`assembly-mates__row assembly-mates__row--${row.state}`}
                  onClick={() => selectMate(row.id === selectedMate ? null : row.id)}
                  data-tip={row.state === 'conflict' ? offText(row, units) : undefined}
                  type="button"
                >
                  <span className="assembly-mates__label">{line}</span>
                  {flagged ? (
                    <span className="assembly-mates__state">{text.states[row.state]}</span>
                  ) : null}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {conflicts > 0 ? (
        <p className="assembly-mates__conflicts" role="status">
          {text.conflicts(conflicts)}
        </p>
      ) : null}
      {picked === null ? null : (
        <MateEditor
          key={`${picked.id}:${picked.kind}:${picked.value}:${picked.aligned}:${picked.suppressed}`}
          name={mateRowText(picked, nameOf)}
          row={picked}
        />
      )}
    </section>
  );
}

/** The picked mate's kind and value, Apply, Suppress and Delete. */
function MateEditor({ row, name }: { row: MateRow; name: string }): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const editMate = useAssemblyStore((s) => s.editMate);
  const deleteMate = useAssemblyStore((s) => s.deleteMate);
  const [kind, setKind] = useState<MateKind>(row.kind);
  const [typed, setTyped] = useState(() => mateValueText(row.kind, row.value, units));
  const [aligned, setAligned] = useState(row.aligned);
  const kindId = useId();
  const alignedId = useId();
  const field = mateValueField(kind);
  const value = field === null ? 0 : parseMateValue(kind, typed, units);
  const kinds = kindsFor(row);
  const applyWhy = value === null ? strings.why.fixValues : undefined;

  const apply = () => {
    if (value === null) {
      return;
    }
    void editMate(row.id, {
      kind,
      ...(field === null ? {} : { value }),
      ...(kind === 'distance' ? { aligned } : {}),
    });
  };

  return (
    <fieldset className="assembly-mates__editor">
      <legend className="assembly-mates__legend">{text.edit(name)}</legend>
      {row.state === 'conflict' ? (
        <p className="assembly-mates__off">{offText(row, units)}</p>
      ) : null}
      <div className="assembly-mates__field">
        <label htmlFor={kindId}>{text.type}</label>
        <select
          className="assembly-mates__select"
          disabled={kinds.length < 2}
          id={kindId}
          onChange={(event) => {
            const next = kinds.find((k) => k === event.target.value) ?? kind;
            // The angle and the gap read differently: start the field afresh.
            if ((next === 'angle') !== (kind === 'angle')) {
              setTyped(mateValueText(next, next === 'angle' ? 90 : 0, units));
            }
            setKind(next);
          }}
          value={kind}
        >
          {kinds.map((k) => (
            <option key={k} value={k}>
              {text.kinds[k]}
            </option>
          ))}
        </select>
      </div>
      {field === null ? null : (
        <NumberField
          error={value === null ? strings.assembly.badValue : undefined}
          label={text[field]}
          onChange={setTyped}
          onEnter={apply}
          unit={field === 'angle' ? '°' : unitSuffix(units)}
          value={typed}
        />
      )}
      {kind === 'distance' ? (
        <div className="assembly-mates__check">
          <input
            checked={aligned}
            id={alignedId}
            onChange={(event) => setAligned(event.target.checked)}
            type="checkbox"
          />
          <label htmlFor={alignedId}>{text.aligned}</label>
        </div>
      ) : null}
      <div className="assembly-mates__buttons">
        <button
          className="assembly-mates__button"
          disabled={applyWhy !== undefined}
          onClick={apply}
          type="button"
          data-tip={whyTip(text.apply, applyWhy)}
        >
          <ButtonFace icon="formApply" label={text.apply} />
        </button>
        <button
          className="assembly-mates__button"
          onClick={() => void editMate(row.id, { suppressed: !row.suppressed })}
          type="button"
          data-tip={row.suppressed ? text.unsuppress : text.suppress}
        >
          <ButtonFace
            icon={row.suppressed ? 'visible' : 'hidden'}
            label={row.suppressed ? text.unsuppress : text.suppress}
          />
        </button>
        <button
          className="assembly-mates__button"
          onClick={() => void deleteMate(row.id)}
          type="button"
          data-tip={text.delete}
        >
          <ButtonFace icon="deleteToolpath" label={text.delete} />
        </button>
      </div>
    </fieldset>
  );
}
