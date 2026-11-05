import { useId, useState, type JSX } from 'react';

import type { TilingMethod } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  clearedTiling,
  convertTilingForm,
  datumBlocksTiling,
  initialTilingForm,
  tilingFromForm,
  usedFields,
  validateTilingForm,
  type TilingField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './ToolpathTilingForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { tiling as tilingPicture } from '../HelperPicture/pictures/toolpaths';
import { HelperRow } from '../HelperRow/HelperRow';

const text = strings.toolpaths.tiling;

const METHODS: ReadonlyArray<Choice<TilingMethod>> = [
  { value: 'individual', label: text.individual },
  { value: 'feedX', label: text.feedX },
  { value: 'feedY', label: text.feedY },
];

export function ToolpathTilingForm(): JSX.Element {
  const tiling = useToolpathStore((store) => store.tiling);
  const origin = useDocumentStore((store) => store.job.xyOrigin);
  const closeForm = useUiStore((store) => store.closeForm);
  const form = useFormState(
    (units) => initialTilingForm(useToolpathStore.getState().tiling, units),
    convertTilingForm,
  );
  const [failure, setFailure] = useState<string | null>(null);
  const tileListId = useId();
  const { state, units } = form;
  const errors = validateTilingForm(state, units);
  const next = tilingFromForm(state, units, tiling);
  const used = usedFields(state.method);
  const blocked = datumBlocksTiling(state.enabled, origin);
  const updateWhy = firstWhy(
    [blocked, text.needsBottomLeft],
    [next === null, strings.why.fixValues],
  );
  const setField = (field: TilingField, value: string) =>
    form.setState((current) => ({ ...current, [field]: value }));

  const update = () => {
    if (next === null) {
      return;
    }
    void useToolpathStore
      .getState()
      .setTiling(next)
      .then((ok) => setFailure(ok ? null : useDocumentStore.getState().lastError));
  };

  const deleteAll = () => {
    void useToolpathStore
      .getState()
      .setTiling(clearedTiling())
      .then((ok) => {
        setFailure(ok ? null : useDocumentStore.getState().lastError);
        if (ok) {
          form.setState(initialTilingForm(null, form.unitsRef.current));
        }
      });
  };

  const tiles = tiling?.enabled === true ? tiling.tiles : [];
  const unit = unitSuffix(units);

  return (
    <div className="toolpath-tiling">
      <p className="toolpath-tiling__hint">{text.hint}</p>
      <CheckField
        checked={state.enabled}
        label={text.enabled}
        onChange={(enabled) => form.setState((current) => ({ ...current, enabled }))}
      />
      {blocked ? (
        <p className="toolpath-tiling__notice" role="alert">
          {text.needsBottomLeft}
        </p>
      ) : null}
      <ChoiceField
        disabled={!state.enabled}
        label={text.method}
        onChange={(method) => form.setState((current) => ({ ...current, method }))}
        options={METHODS}
        value={state.method}
      />
      <fieldset className="toolpath-tiling__group" disabled={!state.enabled}>
        <legend className="toolpath-tiling__legend">{text.sizes}</legend>
        <HelperRow picture={<HelperPicture picture={tilingPicture} />}>
          {used.includes('tileWidth') ? (
            <NumberField
              error={errors.tileWidth}
              label={text.tileWidth}
              onChange={(v) => setField('tileWidth', v)}
              unit={unit}
              value={state.tileWidth}
            />
          ) : null}
          {used.includes('tileHeight') ? (
            <NumberField
              error={errors.tileHeight}
              label={text.tileHeight}
              onChange={(v) => setField('tileHeight', v)}
              unit={unit}
              value={state.tileHeight}
            />
          ) : null}
          <NumberField
            error={errors.overlap}
            label={text.overlap}
            onChange={(v) => setField('overlap', v)}
            unit={unit}
            value={state.overlap}
          />
          <p className="toolpath-tiling__hint">
            {state.method === 'individual' ? text.individualHint : text.feedHint}
          </p>
          <CheckField
            checked={state.smallestFirst}
            label={text.smallestFirst}
            onChange={(smallestFirst) =>
              form.setState((current) => ({ ...current, smallestFirst }))
            }
          />
        </HelperRow>
      </fieldset>
      <button
        className="toolpath-tiling__update"
        disabled={updateWhy !== undefined}
        onClick={update}
        type="button"
        data-tip={whyTip(text.update, updateWhy)}
      >
        <ButtonFace icon="recalculateAll" label={text.update} />
      </button>
      <fieldset className="toolpath-tiling__group">
        <legend className="toolpath-tiling__legend">{text.tiles}</legend>
        {tiles.length === 0 ? (
          <p className="toolpath-tiling__hint">{text.off}</p>
        ) : (
          <>
            <p className="toolpath-tiling__hint" aria-live="polite">
              {text.count(tiles.length)}
            </p>
            <label className="toolpath-tiling__field" htmlFor={tileListId}>
              {text.activeTile}
              <select
                className="toolpath-tiling__select"
                id={tileListId}
                onChange={(event) =>
                  void useToolpathStore.getState().viewTile({ active: Number(event.target.value) })
                }
                value={String(tiling?.active ?? 0)}
              >
                {tiles.map((tile, index) => (
                  <option key={`${tile.row}-${tile.column}`} value={String(index)}>
                    {text.tile(index + 1, tile.row + 1, tile.column + 1)}
                  </option>
                ))}
              </select>
            </label>
          </>
        )}
        <CheckField
          checked={tiling?.originalPosition ?? false}
          disabled={tiles.length === 0}
          label={text.originalPosition}
          onChange={(originalPosition) =>
            void useToolpathStore.getState().viewTile({ originalPosition })
          }
        />
      </fieldset>
      {failure === null ? null : <p className="toolpath-tiling__notice">{failure}</p>}
      <FormButtons
        closeLabel={text.hide}
        onClose={closeForm}
        onSecondary={deleteAll}
        secondaryLabel={text.deleteAll}
      />
    </div>
  );
}
