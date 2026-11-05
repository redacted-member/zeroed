import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type JSX,
} from 'react';

import { api } from '../../bridge/formApi';
import type { SliceArgs, SlicePreview } from '../../bridge/modelTools';
import { strings } from '../../lib/formStrings';
import { runModelAction } from '../../lib/runModelAction';
import { formatLength, unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useSliceViewStore } from '../../state/sliceViewStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SliceHeightBar } from '../SliceHeightBar/SliceHeightBar';
import {
  addHeightAt,
  applyHeight,
  changeSlicing,
  convertSliceModelUnits,
  deleteHeight,
  editingTop,
  heightAtBar,
  heightBands,
  listHeights,
  selectHeight,
  shownHeights,
  sliceBand,
  viewHeightEdit,
  type HeightEdit,
  initialSliceModelState,
  sliceFacts,
  sliceModelArgs,
  validateSliceModel,
  type SliceMode,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './SliceModelForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { slice } from '../HelperPicture/pictures/modeling';

const text = strings.model.slice;

const MODES: ReadonlyArray<Choice<SliceMode>> = [
  { value: 'uniform', label: text.uniform },
  { value: 'count', label: text.count },
];

/** A preview reply, kept with the arguments it answers so a stale one is
 * never shown. */
type PreviewState =
  | { kind: 'none' }
  | { kind: 'ok'; for: SliceArgs; preview: SlicePreview }
  | { kind: 'error'; for: SliceArgs; message: string };

export function SliceModelForm(): JSX.Element {
  const stamp = useModelStore((store) => store.stamp);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialSliceModelState, convertSliceModelUnits);
  const [showErrors, setShowErrors] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [reply, setReply] = useState<PreviewState>({ kind: 'none' });
  const heightListId = useId();
  const heightFieldId = useId();
  const heightErrorId = `${heightFieldId}-error`;
  const [heightError, setHeightError] = useState<string | null>(null);
  const errors = useMemo(() => validateSliceModel(state, units), [state, units]);
  const args = useMemo(() => sliceModelArgs(state, units), [state, units]);
  const unit = unitSuffix(units);
  const materialThickness = useDocumentStore((store) => store.job.thickness);

  useEffect(() => {
    if (args === null) {
      return;
    }
    let live = true;
    api.model
      .slicePreview(args)
      .then((answer) => {
        if (live) {
          setReply({ kind: 'ok', for: args, preview: answer });
        }
      })
      .catch((error: unknown) => {
        if (live) {
          setReply({
            kind: 'error',
            for: args,
            message: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      live = false;
    };
    // The stamp is a dependency so the table follows model changes.
  }, [args, stamp]);

  const preview: PreviewState =
    reply.kind !== 'none' && reply.for === args ? reply : { kind: 'none' };
  const facts =
    preview.kind === 'ok' ? sliceFacts(preview.preview.slabs, preview.preview.maxHeight) : null;
  const shownLength = (mm: number | undefined) =>
    mm === undefined ? '' : `${formatLength(mm, units)} ${unit}`;
  // The last model height known, kept while a new preview is on its way.
  const maxHeight = reply.kind === 'ok' ? reply.preview.maxHeight : 0;
  const shown =
    state.heights ?? (preview.kind === 'ok' ? shownHeights(state, preview.preview.slabs) : []);
  const bands = heightBands(shown, maxHeight);
  const list = listHeights(shown, maxHeight);
  const topPicked = editingTop(state, shown);
  const applyHeightWhy = firstWhy(
    [maxHeight <= 0, strings.why.noModelHeight],
    [topPicked, strings.why.topPicked],
  );
  const deleteHeightWhy = firstWhy(
    [state.activeHeight === null, strings.why.noHeight],
    [topPicked, strings.why.topPicked],
  );
  const edit = (result: HeightEdit) => {
    if (result.ok) {
      setHeightError(null);
      setState(result.state);
    } else {
      setHeightError(result.error);
    }
  };

  // The 3D view shows the picked slice and edits the list while the form
  // is open. The latest values are kept for its edits, which can come
  // several to a frame (wheel notches).
  const latest = useRef({ state, shown, maxHeight, units });
  useLayoutEffect(() => {
    latest.current = { state, shown, maxHeight, units };
  });
  const band = sliceBand(state, shown, maxHeight);
  useEffect(() => {
    const view = useSliceViewStore.getState();
    view.setActive(true);
    return () => useSliceViewStore.getState().setActive(false);
  }, []);
  useEffect(() => {
    useSliceViewStore.getState().setBand(band);
  }, [band]);
  useEffect(
    () =>
      useSliceViewStore.subscribe((store, previous) => {
        const request = store.request;
        if (request === null || request === previous.request) {
          return;
        }
        const now = latest.current;
        const current = now.state.heights ?? now.shown;
        const result = viewHeightEdit(now.state, current, request, now.maxHeight, now.units);
        if (result.ok) {
          latest.current = {
            ...now,
            state: result.state,
            shown: result.state.heights ?? current,
          };
          setHeightError(null);
          setState(result.state);
        } else {
          setHeightError(result.error);
        }
      }),
    [setState],
  );

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    if (args === null) {
      return;
    }
    void runModelAction(() => api.model.slice(args)).then((reply) => {
      if (reply === null) {
        setFailure(useDocumentStore.getState().lastError);
        return;
      }
      setFailure(null);
      useModelStore.getState().select(reply.ids[0] ?? null);
    });
  };

  return (
    <form className="slice-model" onSubmit={onSubmit}>
      <p className="slice-model__hint">{text.hint}</p>
      <dl className="slice-model__facts">
        <dt className="slice-model__term">{text.modelThickness}</dt>
        <dd className="slice-model__value">
          {maxHeight > 0 ? `${formatLength(maxHeight, units)} ${unit}` : ''}
        </dd>
        <dt className="slice-model__term">{text.materialThickness}</dt>
        <dd className="slice-model__value">{`${formatLength(materialThickness, units)} ${unit}`}</dd>
      </dl>
      <fieldset className="slice-model__group">
        <legend className="slice-model__legend">{text.thicknessGroup}</legend>
        <HelperRow picture={<HelperPicture picture={slice} />}>
          <ChoiceField
            label={text.mode}
            onChange={(mode) => setState((current) => changeSlicing(current, { mode }))}
            options={MODES}
            value={state.mode}
          />
          {state.mode === 'uniform' ? (
            <>
              <NumberField
                error={showErrors ? errors.thickness : undefined}
                label={text.thickness}
                onChange={(thickness) =>
                  setState((current) => changeSlicing(current, { thickness }))
                }
                unit={unit}
                value={state.thickness}
              />
              <CheckField
                checked={state.fromTop}
                label={text.fromTop}
                onChange={(fromTop) => setState((current) => changeSlicing(current, { fromTop }))}
              />
              <p className="slice-model__hint">{text.fromTopHint}</p>
              <dl className="slice-model__facts">
                <dt className="slice-model__term">{text.count}</dt>
                <dd className="slice-model__value">{facts?.count ?? ''}</dd>
                <dt className="slice-model__term">{text.topSliceThickness}</dt>
                <dd className="slice-model__value">{shownLength(facts?.partThickness)}</dd>
              </dl>
            </>
          ) : null}
          {state.mode === 'count' ? (
            <>
              <NumberField
                error={showErrors ? errors.count : undefined}
                label={text.countField}
                onChange={(count) => setState((current) => changeSlicing(current, { count }))}
                value={state.count}
              />
              <p className="slice-model__hint">{text.countHint}</p>
              <dl className="slice-model__facts">
                <dt className="slice-model__term">{text.thickness}</dt>
                <dd className="slice-model__value">{shownLength(facts?.sliceThickness)}</dd>
              </dl>
            </>
          ) : null}
          <CheckField
            checked={state.boundaryVectors}
            label={text.boundaryVectors}
            onChange={(boundaryVectors) => setState((current) => ({ ...current, boundaryVectors }))}
          />
          <p className="slice-model__hint">{text.boundaryHint}</p>
        </HelperRow>
      </fieldset>
      <section className="slice-model__group" aria-labelledby={`${heightListId}-title`}>
        <h3 className="slice-model__legend" id={`${heightListId}-title`}>
          {text.heightsGroup}
        </h3>
        <p className="slice-model__hint">{text.heightsHint}</p>
        <div className="slice-model__heights">
          <SliceHeightBar
            active={state.activeHeight}
            bands={bands}
            label={text.heightBar}
            onAddAt={(fromBottom) =>
              edit(addHeightAt(state, shown, heightAtBar(fromBottom, maxHeight), maxHeight, units))
            }
          />
          <div className="slice-model__height-edit">
            <label className="slice-model__field" htmlFor={heightListId}>
              {text.heightList}
              <select
                className="slice-model__list"
                id={heightListId}
                onChange={(event) => {
                  const index = Number(event.target.value);
                  setHeightError(null);
                  setState((current) =>
                    selectHeight(current, shown, index < 0 ? null : index, units, maxHeight),
                  );
                }}
                size={6}
                value={String(state.activeHeight ?? -1)}
              >
                <option value="-1">{text.newHeight}</option>
                {list
                  .map((height, index) => ({ height, index }))
                  .reverse()
                  .map(({ height, index }) => (
                    <option key={index} value={String(index)}>
                      {index === shown.length
                        ? text.modelTop(`${formatLength(height, units)} ${unit}`)
                        : `${formatLength(height, units)} ${unit}`}
                    </option>
                  ))}
              </select>
            </label>
            <label className="slice-model__field" htmlFor={heightFieldId}>
              {text.heightField(unit)}
              <input
                aria-describedby={heightError === null ? undefined : heightErrorId}
                aria-invalid={heightError !== null}
                className="slice-model__input"
                id={heightFieldId}
                onChange={(event) =>
                  setState((current) => ({ ...current, heightText: event.target.value }))
                }
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    event.preventDefault();
                    edit(applyHeight(state, shown, maxHeight, units));
                  }
                }}
                type="text"
                value={state.heightText}
              />
            </label>
            <div className="slice-model__height-buttons">
              <button
                className="slice-model__button"
                disabled={applyHeightWhy !== undefined}
                onClick={() => edit(applyHeight(state, shown, maxHeight, units))}
                type="button"
                data-tip={whyTip(text.apply, applyHeightWhy)}
              >
                <ButtonFace icon="formApply" label={text.apply} />
              </button>
              <button
                className="slice-model__button"
                disabled={deleteHeightWhy !== undefined}
                onClick={() => {
                  setHeightError(null);
                  setState(deleteHeight(state, shown));
                }}
                type="button"
                data-tip={whyTip(text.delete, deleteHeightWhy)}
              >
                <ButtonFace icon="deleteToolpath" label={text.delete} />
              </button>
            </div>
            {heightError === null ? null : (
              <span className="slice-model__error" id={heightErrorId} role="alert">
                {heightError}
              </span>
            )}
          </div>
        </div>
      </section>
      {preview.kind === 'error' ? <p className="slice-model__notice">{preview.message}</p> : null}
      {failure === null ? null : (
        <p className="slice-model__notice" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [args === null, strings.why.fixValues],
          [preview.kind === 'error', preview.kind === 'error' ? preview.message : ''],
          [preview.kind !== 'ok', strings.why.previewing],
        )}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
