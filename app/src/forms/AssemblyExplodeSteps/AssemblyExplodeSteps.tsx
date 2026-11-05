import { useId, useState, type JSX } from 'react';

import { pieceKey, type ExplodeStep, type PieceRef } from '../../bridge/assembly';
import { strings } from '../../lib/formStrings';
import { assemblyFitStrings } from '../../lib/strings/assemblyFit';
import { formatLength, parseLength, toMillimetres, unitSuffix } from '../../lib/units';
import { useAssemblyFitStore } from '../../state/assemblyFitStore';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { pieceLabel } from '../AssemblyViewForm/model';
import { NumberField } from '../NumberField/NumberField';
import { directionId, moveStep, newStep, STEP_DIRECTIONS, withoutPiece, withPiece } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { moveWhy } from '../ButtonFace/why';

import './AssemblyExplodeSteps.scss';

const text = assemblyFitStrings.steps;
const ALONG = 'along';

/**
 * Named explode steps (as exploded view steps are made elsewhere): New
 * Step starts one for the picked piece, each step's pieces, direction and
 * distance can be changed, and steps move up and down the list, which is
 * the order the Explode slider plays them in. Each change is one undo
 * step kept with the job.
 */
export function AssemblyExplodeSteps(): JSX.Element {
  const steps = useAssemblyStore((s) => s.explodeSteps);
  const selected = useAssemblyStore((s) => s.selected);
  const pieces = useAssemblyStore((s) => s.pieces);
  const setSteps = useAssemblyStore((s) => s.setExplodeSteps);
  const offsets = useAssemblyFitStore((s) => s.offsets);
  const [open, setOpen] = useState<number | null>(null);
  const picked = pieces.find((p) => pieceKey(p) === selected) ?? null;
  const nameOf = (ref: PieceRef) => {
    const row = pieces.find((p) => pieceKey(p) === pieceKey(ref));
    return row === undefined ? pieceKey(ref) : pieceLabel(row);
  };
  const current = open === null ? null : (steps[open] ?? null);

  const add = async () => {
    if (picked === null) {
      return;
    }
    const kept = await setSteps(
      [...steps, newStep(steps, picked, offsets[pieceKey(picked)])],
      text.undo.add,
    );
    if (kept !== null) {
      setOpen(kept.length - 1);
    }
  };

  const newWhy = picked === null ? text.pickFirst : undefined;
  return (
    <div className="assembly-steps">
      <h4 className="assembly-steps__heading">{text.title}</h4>
      <p className="assembly-steps__hint">{text.hint}</p>
      {steps.length === 0 ? null : (
        <ol className="assembly-steps__list">
          {steps.map((step, index) => (
            <li className="assembly-steps__item" key={step.id}>
              <button
                aria-pressed={open === index}
                className="assembly-steps__name"
                onClick={() => setOpen(open === index ? null : index)}
                type="button"
              >
                {`${index + 1}. ${step.name}`}
              </button>
              <button
                aria-label={`${text.up}: ${step.name}`}
                className="assembly-steps__move"
                disabled={moveWhy(index, steps.length, -1) !== undefined}
                onClick={() => {
                  void setSteps(moveStep(steps, index, -1), text.undo.move);
                  setOpen(index - 1);
                }}
                data-tip={whyTip(text.up, moveWhy(index, steps.length, -1))}
                type="button"
              >
                <ButtonFace icon="arrowUp" />
              </button>
              <button
                aria-label={`${text.down}: ${step.name}`}
                className="assembly-steps__move"
                disabled={moveWhy(index, steps.length, 1) !== undefined}
                onClick={() => {
                  void setSteps(moveStep(steps, index, 1), text.undo.move);
                  setOpen(index + 1);
                }}
                data-tip={whyTip(text.down, moveWhy(index, steps.length, 1))}
                type="button"
              >
                <ButtonFace icon="arrowDown" />
              </button>
            </li>
          ))}
        </ol>
      )}
      <button
        className="assembly-steps__button"
        disabled={newWhy !== undefined}
        onClick={() => void add()}
        data-tip={whyTip(text.newStep, newWhy)}
        type="button"
      >
        <ButtonFace icon="formCreate" label={text.newStep} />
      </button>
      {current === null || open === null ? null : (
        <StepEditor
          index={open}
          key={`${current.id}:${current.name}:${current.distance}:${current.direction.join()}`}
          nameOf={nameOf}
          onDone={() => setOpen(null)}
          picked={picked}
          step={current}
          steps={steps}
        />
      )}
    </div>
  );
}

interface StepEditorProps {
  index: number;
  step: ExplodeStep;
  steps: readonly ExplodeStep[];
  picked: PieceRef | null;
  nameOf: (ref: PieceRef) => string;
  onDone: () => void;
}

/** One step's name, direction, distance and pieces. */
function StepEditor({ index, step, steps, picked, nameOf, onDone }: StepEditorProps): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const setSteps = useAssemblyStore((s) => s.setExplodeSteps);
  const [name, setName] = useState(step.name);
  const [direction, setDirection] = useState(directionId(step.direction) ?? ALONG);
  const [distance, setDistance] = useState(() => formatLength(step.distance, units));
  const nameId = useId();
  const directionFieldId = useId();
  const typed = parseLength(distance);
  const valid = typed !== null && Number.isFinite(typed);
  const pickedIn = picked !== null && step.pieces.some((p) => pieceKey(p) === pieceKey(picked));
  const addWhy = firstWhy([picked === null, text.pickFirst], [pickedIn, strings.why.pieceIn]);
  const applyWhy = valid ? undefined : strings.why.fixValues;

  const apply = () => {
    if (!valid) {
      return;
    }
    const axis = STEP_DIRECTIONS.find((d) => d.id === direction);
    const changed: ExplodeStep = {
      ...step,
      name,
      direction: axis === undefined ? step.direction : axis.vector,
      distance: toMillimetres(typed, units),
    };
    void setSteps(
      steps.map((s, i) => (i === index ? changed : s)),
      text.undo.edit,
    );
  };

  return (
    <fieldset className="assembly-steps__editor">
      <legend className="assembly-steps__legend">{text.edit(step.name)}</legend>
      <div className="assembly-steps__field">
        <label htmlFor={nameId}>{text.name}</label>
        <input
          className="assembly-steps__input"
          id={nameId}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => event.key === 'Enter' && apply()}
          type="text"
          value={name}
        />
      </div>
      <div className="assembly-steps__field">
        <label htmlFor={directionFieldId}>{text.direction}</label>
        <select
          className="assembly-steps__input"
          id={directionFieldId}
          onChange={(event) => setDirection(event.target.value)}
          value={direction}
        >
          {directionId(step.direction) === null ? (
            <option value={ALONG}>{text.alongJoints}</option>
          ) : null}
          {STEP_DIRECTIONS.map((d) => (
            <option key={d.id} value={d.id}>
              {d.label}
            </option>
          ))}
        </select>
      </div>
      <NumberField
        error={valid ? undefined : strings.assembly.badValue}
        label={text.distance}
        onChange={setDistance}
        onEnter={apply}
        unit={unitSuffix(units)}
        value={distance}
      />
      <p className="assembly-steps__label">{text.pieces}</p>
      <ul className="assembly-steps__pieces">
        {step.pieces.map((piece) => {
          const pieceName = nameOf(piece);
          return (
            <li className="assembly-steps__piece" key={pieceKey(piece)}>
              <span className="assembly-steps__piece-name">{pieceName}</span>
              <button
                aria-label={text.removePiece(pieceName)}
                className="assembly-steps__move"
                onClick={() => {
                  const next = withoutPiece(steps, index, pieceKey(piece));
                  void setSteps(
                    next,
                    next.length < steps.length ? text.undo.remove : text.undo.edit,
                  );
                  if (next.length < steps.length) {
                    onDone();
                  }
                }}
                data-tip={text.removePiece(pieceName)}
                type="button"
              >
                <ButtonFace icon="itemRemove" />
              </button>
            </li>
          );
        })}
      </ul>
      <div className="assembly-steps__buttons">
        <button
          className="assembly-steps__button"
          disabled={addWhy !== undefined}
          onClick={() =>
            picked !== null && void setSteps(withPiece(steps, index, picked), text.undo.edit)
          }
          type="button"
          data-tip={whyTip(text.addPiece, addWhy)}
        >
          <ButtonFace icon="formCreate" label={text.addPiece} />
        </button>
        <button
          className="assembly-steps__button"
          disabled={applyWhy !== undefined}
          onClick={apply}
          type="button"
          data-tip={whyTip(text.apply, applyWhy)}
        >
          <ButtonFace icon="formApply" label={text.apply} />
        </button>
        <button
          className="assembly-steps__button"
          onClick={() => {
            void setSteps(
              steps.filter((_, i) => i !== index),
              text.undo.remove,
            );
            onDone();
          }}
          type="button"
          data-tip={text.remove}
        >
          <ButtonFace icon="itemRemove" label={text.remove} />
        </button>
      </div>
    </fieldset>
  );
}
