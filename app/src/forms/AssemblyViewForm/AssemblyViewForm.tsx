import { useEffect, useId, useState, type JSX } from 'react';

import { MATE_KINDS, pieceKey, type PieceRef, type PieceRow } from '../../bridge/assembly';
import { strings } from '../../lib/formStrings';
import { assemblyMatesStrings } from '../../lib/strings/assemblyMates';
import { formatLength, unitSuffix } from '../../lib/units';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { AssemblyAddComponent } from '../AssemblyAddComponent/AssemblyAddComponent';
import { AssemblyExplode } from '../AssemblyExplode/AssemblyExplode';
import { AssemblyGuide } from '../AssemblyGuide/AssemblyGuide';
import { AssemblyFurniture } from '../AssemblyFurniture/AssemblyFurniture';
import { AssemblyExport } from '../AssemblyExport/AssemblyExport';
import { AssemblyFit } from '../AssemblyFit/AssemblyFit';
import { AssemblyMates } from '../AssemblyMates/AssemblyMates';
import { mateValueField, mateValueText, parseMateValue } from '../AssemblyMates/model';
import { AssemblyMeasure } from '../AssemblyMeasure/AssemblyMeasure';
import { AssemblyPieces } from '../AssemblyPieces/AssemblyPieces';
import { AssemblySection } from '../AssemblySection/AssemblySection';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { exportCutList } from './exportCutList';
import {
  groupCutList,
  MATE_TURNS,
  shownSections,
  parsePlacement,
  pieceLabel,
  placementText,
  type PlacementText,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './AssemblyViewForm.scss';

const text = strings.assembly;
const mateText = assemblyMatesStrings.mates;
const AXES = ['X', 'Y', 'Z'] as const;

/**
 * The Assembly View, in the Toolpaths panel: the pieces (each part a
 * sheet's toolpaths cut free, from the toolpath simulation, or a sheet's
 * raw model) shown put
 * together in the 3D view. A piece is picked in the list or the view and
 * placed with the handles in the view, by value here, or by laying one of
 * its faces against a face of another piece. Verify Fit measures the
 * joints on the pieces' exact cut edges; the cut list works on the pieces
 * as they stand.
 */
export function AssemblyViewForm(): JSX.Element {
  const closeForm = useUiStore((ui) => ui.closeForm);
  const pieces = useAssemblyStore((s) => s.pieces);
  const selected = useAssemblyStore((s) => s.selected);
  const error = useAssemblyStore((s) => s.error);
  const notice = useAssemblyStore((s) => s.notice);
  const snap = useAssemblyStore((s) => s.snap);
  const setSnap = useAssemblyStore((s) => s.setSnap);
  const loading = useAssemblyStore((s) => s.loading);
  const snapId = useId();

  // Opening the form shows the pieces in the 3D view; closing it gives the
  // view back to the job.
  useEffect(() => {
    const ui = useUiStore.getState();
    if (ui.viewMode === '2d') {
      ui.setViewMode('3d');
    }
    void useAssemblyStore.getState().open();
    return () => useAssemblyStore.getState().close();
  }, []);

  const piece = pieces.find((p) => pieceKey(p) === selected) ?? null;
  // Before the first piece only the hint, the list and Add Component can
  // be used; the working sections come with the pieces.
  const shown = new Set<string>(shownSections(pieces.length));

  return (
    <div className="assembly-view">
      <AssemblyGuide empty={pieces.length === 0 && !loading} />
      <AssemblyFurniture />
      <p className="assembly-view__hint">{text.hint}</p>
      <div className="assembly-view__check">
        <input
          checked={snap}
          id={snapId}
          onChange={(event) => setSnap(event.target.checked)}
          type="checkbox"
        />
        <label htmlFor={snapId}>{text.snap}</label>
      </div>
      <AssemblyPieces />
      {piece !== null && piece.component !== undefined && piece.component !== null ? (
        <RemoveComponent component={piece.component} />
      ) : null}
      <AssemblyExplode />
      {shown.has('mates') ? <AssemblyMates /> : null}
      <AssemblyAddComponent />
      {piece !== null && piece.hasModel && piece.hasToolpaths ? (
        <PieceSource piece={piece} />
      ) : null}
      {!shown.has('placement') ? null : piece === null ? (
        <p className="assembly-view__empty">{text.noneSelected}</p>
      ) : (
        <Placement
          key={`${pieceKey(piece)}:${piece.position.join()}:${piece.rotation.join()}`}
          piece={piece}
        />
      )}
      {shown.has('matePieces') ? <MatePieces /> : null}
      {shown.has('fit') ? <AssemblyFit /> : null}
      {shown.has('section') ? <AssemblySection /> : null}
      {shown.has('measure') ? <AssemblyMeasure /> : null}
      {shown.has('cutList') ? <CutList /> : null}
      {error === null ? null : (
        <p className="assembly-view__error" role="alert">
          {error}
        </p>
      )}
      {notice === null ? null : (
        <p aria-live="polite" className="assembly-view__status">
          {notice}
        </p>
      )}
      <FormButtons onClose={closeForm} />
    </div>
  );
}

/** Takes the picked component piece out of the view. */
function RemoveComponent({ component }: { component: number }): JSX.Element {
  const removeComponent = useAssemblyStore((s) => s.removeComponent);
  return (
    <button
      className="assembly-view__button"
      onClick={() => void removeComponent(component)}
      type="button"
      data-tip={text.removePiece}
    >
      <ButtonFace icon="itemRemove" label={text.removePiece} />
    </button>
  );
}

/** Whether the picked piece's sheet gives its raw model or its machined parts. */
function PieceSource({ piece }: { piece: PieceRow }): JSX.Element {
  const setSource = useAssemblyStore((s) => s.setSource);
  const name = useId();
  const options = [
    { model: false, label: text.sourceSimulation },
    { model: true, label: text.sourceModel },
  ];
  return (
    <fieldset className="assembly-view__section">
      <legend className="assembly-view__heading">{text.source}</legend>
      <p className="assembly-view__hint">{text.sourceHint}</p>
      {options.map((option) => {
        const id = `${name}-${option.model ? 'model' : 'simulation'}`;
        return (
          <div className="assembly-view__check" key={id}>
            <input
              checked={piece.model === option.model}
              id={id}
              name={name}
              onChange={() => void setSource(piece.sheet, option.model)}
              type="radio"
            />
            <label htmlFor={id}>{option.label}</label>
          </div>
        );
      })}
    </fieldset>
  );
}

/** The picked piece's place by value: position in the job unit, turns in degrees. */
function Placement({ piece }: { piece: PieceRow }): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const place = useAssemblyStore((s) => s.place);
  const [fields, setFields] = useState<PlacementText>(() => placementText(piece, units));
  const grounded = piece.grounded === true;
  const [bad, setBad] = useState<number[]>([]);
  const unit = unitSuffix(units);

  const apply = () => {
    const parsed = parsePlacement(fields, units);
    if (!parsed.ok) {
      setBad(parsed.bad);
      return;
    }
    setBad([]);
    void place(pieceKey(piece), parsed.position, parsed.rotation);
  };

  const field = (group: 'position' | 'rotation', index: 0 | 1 | 2) => {
    const at = group === 'position' ? index : index + 3;
    return (
      <NumberField
        error={bad.includes(at) ? text.badValue : undefined}
        key={`${group}-${index}`}
        label={text.axisLabel(group === 'position' ? text.position : text.rotation, AXES[index])}
        onChange={(value) => {
          const next: PlacementText = {
            position: [...fields.position],
            rotation: [...fields.rotation],
          };
          next[group][index] = value;
          setFields(next);
        }}
        onEnter={apply}
        step={group === 'position' ? 1 : 15}
        unit={group === 'position' ? unit : '°'}
        value={fields[group][index]}
      />
    );
  };

  return (
    <fieldset className="assembly-view__section" disabled={grounded}>
      <legend className="assembly-view__heading">{`${text.placement}: ${pieceLabel(piece)}`}</legend>
      {grounded ? (
        <p className="assembly-view__hint">{assemblyMatesStrings.tree.groundedHint}</p>
      ) : null}
      <div className="assembly-view__grid">
        {field('position', 0)}
        {field('position', 1)}
        {field('position', 2)}
        {field('rotation', 0)}
        {field('rotation', 1)}
        {field('rotation', 2)}
      </div>
      <button className="assembly-view__button" onClick={apply} type="button" data-tip={text.apply}>
        <ButtonFace icon="formApply" label={text.apply} />
      </button>
    </fieldset>
  );
}

/**
 * Mate Pieces: pick a type (Against, Flush, Distance, Angle, Concentric),
 * then click the face (or round side) that stays and one on another piece,
 * which moves to meet it; the mate is kept and solved with the others (one
 * undo step). The face under the pointer is lit meanwhile. Flip takes the
 * moving piece's other side; the value, flip and turn change the mate
 * just made when applied again.
 */
function MatePieces(): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const pieces = useAssemblyStore((s) => s.pieces);
  const mate = useAssemblyStore((s) => s.mate);
  const startMate = useAssemblyStore((s) => s.startMate);
  const cancelMate = useAssemblyStore((s) => s.cancelMate);
  const setMateOptions = useAssemblyStore((s) => s.setMateOptions);
  const applyMate = useAssemblyStore((s) => s.applyMate);
  const [offset, setOffset] = useState(() => mateValueText(mate.kind, mate.offset, units));
  const flipId = useId();
  const turnId = useId();
  const kindId = useId();
  const field = mateValueField(mate.kind);
  const typed = field === null ? 0 : parseMateValue(mate.kind, offset, units);
  const valid = typed !== null;
  const round = mate.kind === 'concentric';
  const against = mate.kind === 'against';
  const nameOf = (ref: PieceRef) => {
    const row = pieces.find((p) => pieceKey(p) === pieceKey(ref));
    return row === undefined ? pieceKey(ref) : pieceLabel(row);
  };
  const done = mate.step === null && mate.target !== null && mate.moving !== null;
  const applyWhy = firstWhy([!done, strings.why.pickFaces], [!valid, strings.why.fixValues]);

  return (
    <fieldset className="assembly-view__section">
      <legend className="assembly-view__heading">{text.mate}</legend>
      <div className="assembly-view__field">
        <label htmlFor={kindId}>{mateText.type}</label>
        <select
          className="assembly-view__select"
          disabled={mate.step !== null || done}
          id={kindId}
          onChange={(event) => {
            const kind = MATE_KINDS.find((k) => k === event.target.value) ?? mate.kind;
            const value = kind === 'angle' ? 90 : 0;
            setOffset(mateValueText(kind, value, units));
            setMateOptions({ kind, offset: value, flip: false, turn: null });
          }}
          value={mate.kind}
        >
          {MATE_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {mateText.kinds[kind]}
            </option>
          ))}
        </select>
      </div>
      <p aria-live="polite" className="assembly-view__status">
        {mate.step === 'target'
          ? round
            ? mateText.pickRoundTarget
            : text.pickTarget
          : mate.step === 'moving'
            ? round
              ? mateText.pickRoundMoving
              : text.pickMoving
            : done && mate.moving !== null && mate.target !== null
              ? text.mated(nameOf(mate.moving), nameOf(mate.target))
              : mateText.kindHints[mate.kind]}
      </p>
      {field === null ? null : (
        <NumberField
          error={valid ? undefined : text.badValue}
          label={mateText[field]}
          onChange={(next) => {
            setOffset(next);
            const value = parseMateValue(mate.kind, next, units);
            if (value !== null) {
              setMateOptions({ offset: value });
            }
          }}
          onEnter={() => done && valid && void applyMate()}
          unit={field === 'angle' ? '°' : unitSuffix(units)}
          value={offset}
        />
      )}
      {round ? null : (
        <div className="assembly-view__check">
          <input
            checked={mate.flip}
            id={flipId}
            onChange={(event) => setMateOptions({ flip: event.target.checked })}
            type="checkbox"
          />
          <label htmlFor={flipId}>{text.flip}</label>
        </div>
      )}
      {against ? (
        <div className="assembly-view__field">
          <label htmlFor={turnId}>{text.turn}</label>
          <select
            className="assembly-view__select"
            id={turnId}
            onChange={(event) => setMateOptions({ turn: Number(event.target.value) })}
            value={mate.turn === null ? '' : String(mate.turn)}
          >
            {mate.turn === null ? <option value="">-</option> : null}
            {MATE_TURNS.map((turn) => (
              <option key={turn} value={String(turn)}>
                {`${turn}°`}
              </option>
            ))}
          </select>
        </div>
      ) : null}
      <div className="assembly-view__row">
        {mate.step === null ? (
          <button
            className="assembly-view__button"
            onClick={startMate}
            type="button"
            data-tip={text.pickFaces}
          >
            <ButtonFace icon="vectorSelection" label={text.pickFaces} />
          </button>
        ) : (
          <button
            className="assembly-view__button"
            onClick={cancelMate}
            type="button"
            data-tip={text.cancel}
          >
            <ButtonFace icon="dismiss" label={text.cancel} />
          </button>
        )}
        <button
          className="assembly-view__button"
          disabled={applyWhy !== undefined}
          onClick={() => void applyMate()}
          type="button"
          data-tip={whyTip(text.apply, applyWhy)}
        >
          <ButtonFace icon="formApply" label={text.apply} />
        </button>
      </div>
    </fieldset>
  );
}

/** Each piece's finished size in the job unit, and Export CSV. */
function CutList(): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const rows = useAssemblyStore((s) => s.cutList);
  const loadCutList = useAssemblyStore((s) => s.loadCutList);
  const length = (mm: number | null) => (mm === null ? '' : formatLength(mm, units));
  const exportWhy = rows === null || rows.length === 0 ? strings.why.noCutList : undefined;

  return (
    <fieldset className="assembly-view__section">
      <legend className="assembly-view__heading">{text.cutList}</legend>
      {rows === null ? null : rows.length === 0 ? (
        <p className="assembly-view__empty">{text.cutListEmpty}</p>
      ) : (
        <div className="assembly-view__scroll">
          <table className="assembly-view__table">
            <thead>
              <tr>
                <th scope="col">{text.columns.piece}</th>
                <th scope="col">{text.columns.count}</th>
                <th scope="col">{text.columns.length}</th>
                <th scope="col">{text.columns.width}</th>
                <th scope="col">{text.columns.thickness}</th>
              </tr>
            </thead>
            <tbody>
              {groupCutList(rows).map((row) => (
                <tr key={row.key}>
                  <th scope="row">{row.label}</th>
                  <td className="assembly-view__number">{row.count}</td>
                  <td className="assembly-view__number">{length(row.length)}</td>
                  <td className="assembly-view__number">{length(row.width)}</td>
                  <td className="assembly-view__number">{length(row.thickness)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="assembly-view__row">
        <button
          className="assembly-view__button"
          onClick={() => void loadCutList()}
          type="button"
          data-tip={rows === null ? text.cutList : text.refresh}
        >
          <ButtonFace
            icon={rows === null ? 'jobSheet' : 'recalculateAll'}
            label={rows === null ? text.cutList : text.refresh}
          />
        </button>
        <button
          className="assembly-view__button"
          disabled={exportWhy !== undefined}
          onClick={() => void exportCutList(rows ?? [])}
          type="button"
          data-tip={whyTip(text.exportCsv, exportWhy)}
        >
          <ButtonFace icon="exportFile" label={text.exportCsv} />
        </button>
      </div>
      <AssemblyExport />
    </fieldset>
  );
}
