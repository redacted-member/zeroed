import { lazy, Suspense, useId, useState, type JSX, type ReactNode } from 'react';

import { strings } from '../../lib/formStrings';
import { saveShadedImage } from '../../lib/shadedImage';
import { isDoubleSided } from '../../lib/sides';
import { formatDuration } from '../../lib/toolpathFields';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { usePreviewStore } from '../../state/previewStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { PreviewChecks } from '../PreviewChecks/PreviewChecks';
import { SliderField } from '../SliderField/SliderField';
import {
  areaChoice,
  fromAreaChoice,
  selectedColour,
  SPEED_MAX,
  SPEED_MIN,
  visibleOnSides,
  type AreaChoice,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './PreviewToolpathsForm.scss';

// The dialog loads the first time the palette button opens it.
const MaterialAppearanceDialog = lazy(async () => ({
  default: (await import('../../panels/MaterialAppearanceDialog/MaterialAppearanceDialog'))
    .MaterialAppearanceDialog,
}));

const text = strings.toolpaths.preview;

export function PreviewToolpathsForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const job = useDocumentStore((store) => store.job);
  const applyJob = useDocumentStore((store) => store.applyJob);
  const defaults = usePrefsStore((store) => store.prefs.view3d);
  const toolpaths = useToolpathStore((store) => store.toolpaths);
  const selectedId = useToolpathStore((store) => store.selectedId);
  const preview = usePreviewStore();
  const names = new Map(toolpaths.map((t) => [t.id, t.name]));
  const radioName = useId();
  const [appearance, setAppearance] = useState(false);
  const units = job.units;
  const busy = preview.running || preview.playing;
  // A double sided job previews the side shown, both with Multi Sided View.
  const activeSide = useDocumentStore((store) => store.activeSide);
  const doubleSided = isDoubleSided(job);
  const bothSides = doubleSided && defaults.multiSided;
  const previewVisible = () => {
    if (doubleSided) {
      void preview.run(visibleOnSides(toolpaths, activeSide, bothSides));
    } else {
      void preview.run();
    }
  };
  const tiling = useToolpathStore((state) => state.tiling);
  const sheetName = useDocumentStore((store) => store.sheets.find((s) => s.active)?.name ?? null);
  const choice = areaChoice(preview.fill, preview.lithophane);
  const choose = (next: AreaChoice) => {
    const picked = fromAreaChoice(next);
    preview.setFill(picked.fill);
    preview.setLithophane(picked.lithophane);
  };
  const swatch = selectedColour(toolpaths, selectedId, preview.toolpathColours);

  const areaRow = (value: AreaChoice, label: string, extra?: ReactNode) => (
    <div className="preview-toolpaths__area">
      <label className="preview-toolpaths__area-label">
        <input
          checked={choice === value}
          name={radioName}
          onChange={() => choose(value)}
          type="radio"
        />
        {label}
      </label>
      {extra === undefined ? null : <span className="preview-toolpaths__area-extra">{extra}</span>}
    </div>
  );

  const setAllWhy = firstWhy(
    [choice !== 'toolpath', strings.why.chooseFirst(text.fillToolpath)],
    [swatch === null, strings.why.pickToolpath],
  );
  const selectedWhy = firstWhy(
    [busy, strings.why.busy],
    [selectedId === null, strings.why.pickToolpath],
  );
  const playWhy = firstWhy(
    [!preview.animating, strings.why.notAnimating],
    [preview.playing, strings.why.alreadyPlaying],
  );
  const pauseWhy = preview.playing ? undefined : strings.why.notPlaying;
  const stepWhy = firstWhy(
    [!preview.animating, strings.why.notAnimating],
    [preview.playing, strings.why.playing],
  );
  const allSidesWhy = firstWhy(
    [!doubleSided, strings.why.notDoubleSided],
    [busy, strings.why.busy],
    [toolpaths.length === 0, strings.why.noToolpaths],
    [visibleOnSides(toolpaths, activeSide, true).length === 0, strings.why.noVisibleToolpaths],
  );
  const allWhy = firstWhy(
    [busy, strings.why.busy],
    [toolpaths.length === 0, strings.why.noToolpaths],
  );
  const visibleWhy = firstWhy(
    [busy, strings.why.busy],
    [toolpaths.length === 0, strings.why.noToolpaths],
    [
      doubleSided
        ? visibleOnSides(toolpaths, activeSide, bothSides).length === 0
        : !toolpaths.some((t) => t.visible),
      strings.why.noVisibleToolpaths,
    ],
  );
  const resetWhy = firstWhy(
    [preview.running, strings.why.busy],
    [preview.heightfield === null, strings.why.noPreview],
  );
  const undoWhy = firstWhy(
    [busy || preview.animating, strings.why.busy],
    [!preview.canUndo, strings.why.nothingToUndo],
  );
  const imageWhy = preview.heightfield === null ? strings.why.noPreview : undefined;

  return (
    <div className="preview-toolpaths">
      <div className="preview-toolpaths__sheet">
        <span className="preview-toolpaths__readout">{text.activeSheetLabel}</span>
        <span className="preview-toolpaths__sheet-name">{sheetName ?? ''}</span>
        <button
          aria-label={text.appearanceOpen}
          className="preview-toolpaths__palette"
          onClick={() => setAppearance(true)}
          data-tip={text.appearanceOpen}
          type="button"
        >
          <svg aria-hidden="true" className="preview-toolpaths__palette-icon" viewBox="0 0 16 16">
            <path d="M8 1.5a6.5 6.5 0 1 0 0 13c1 0 1.5-.6 1.5-1.3 0-.9-.8-1.2-.8-2 0-.7.6-1.2 1.4-1.2h1.6c1.9 0 3.3-1.4 3.3-3.3C15 3.9 11.9 1.5 8 1.5Z" />
            <circle className="preview-toolpaths__palette-dot" cx="4.5" cy="7" r="1.2" />
            <circle className="preview-toolpaths__palette-dot" cx="6.5" cy="4" r="1.2" />
            <circle className="preview-toolpaths__palette-dot" cx="10" cy="4" r="1.2" />
          </svg>
        </button>
      </div>
      {tiling !== null && tiling.enabled && tiling.tiles.length > 0 ? (
        <p className="preview-toolpaths__hint" role="status">
          {text.tileOnly(tiling.active + 1, tiling.tiles.length)}
        </p>
      ) : null}
      <fieldset className="preview-toolpaths__group">
        <legend className="preview-toolpaths__legend">{text.machined}</legend>
        {areaRow('material', text.fillMaterial)}
        {areaRow(
          'surface',
          text.fillSurface,
          <input
            aria-label={text.surfaceColour}
            className="preview-toolpaths__colour"
            disabled={choice !== 'surface'}
            onChange={(event) => preview.setSurfaceColour(event.target.value)}
            type="color"
            value={preview.surfaceColour}
          />,
        )}
        {areaRow(
          'global',
          text.fillGlobal,
          <input
            aria-label={text.fillColour}
            className="preview-toolpaths__colour"
            disabled={choice !== 'global'}
            onChange={(event) => preview.setFillColour(event.target.value)}
            type="color"
            value={preview.fillColour}
          />,
        )}
        {areaRow(
          'toolpath',
          text.fillToolpath,
          <>
            <input
              aria-label={text.selectedColour}
              className="preview-toolpaths__colour"
              disabled={choice !== 'toolpath' || swatch === null || swatch === 'none'}
              onChange={(event) =>
                selectedId !== null && preview.setToolpathColour(selectedId, event.target.value)
              }
              type="color"
              value={swatch === null || swatch === 'none' ? '#000000' : swatch}
            />
            <button
              className="preview-toolpaths__small"
              disabled={setAllWhy !== undefined}
              onClick={() =>
                swatch !== null &&
                preview.setAllToolpathColours(
                  toolpaths.map((t) => t.id),
                  swatch,
                )
              }
              data-tip={whyTip(text.setAllTitle, setAllWhy)}
              type="button"
            >
              <ButtonFace icon="setValue" label={text.setAll} />
            </button>
          </>,
        )}
        {choice === 'toolpath' ? (
          <CheckField
            checked={swatch === 'none'}
            disabled={selectedId === null}
            label={text.noFill}
            onChange={(off) =>
              selectedId !== null &&
              preview.setToolpathColour(
                selectedId,
                off ? 'none' : (selectedColour(toolpaths, selectedId, {}) ?? '#d9534f'),
              )
            }
          />
        ) : null}
        {areaRow(
          'lithophane',
          text.lithophane,
          <SliderField
            disabled={choice !== 'lithophane'}
            label={text.lithoStrength}
            max={100}
            min={0}
            onChange={preview.setLithoStrength}
            value={preview.lithoStrength}
          />,
        )}
      </fieldset>
      <div className="preview-toolpaths__checks">
        <CheckField checked={preview.animate} label={text.animate} onChange={preview.setAnimate} />
        <CheckField
          checked={preview.drawTool}
          disabled={!preview.animate}
          label={text.drawTool}
          onChange={preview.setDrawTool}
        />
      </div>
      <button
        className="preview-toolpaths__button"
        disabled={selectedWhy !== undefined}
        onClick={() => selectedId !== null && void preview.run([selectedId], false, true)}
        type="button"
        data-tip={whyTip(text.selected, selectedWhy)}
      >
        <ButtonFace icon="preview" label={text.selected} />
      </button>
      <div className="preview-toolpaths__controls" role="group" aria-label={text.controls}>
        <button
          aria-label={text.play}
          className="preview-toolpaths__button"
          disabled={playWhy !== undefined}
          onClick={preview.resume}
          data-tip={whyTip(text.play, playWhy)}
          type="button"
        >
          <ButtonFace icon="play" label={text.play} />
        </button>
        <button
          aria-label={text.pause}
          className="preview-toolpaths__button"
          disabled={pauseWhy !== undefined}
          onClick={preview.pause}
          data-tip={whyTip(text.pause, pauseWhy)}
          type="button"
        >
          <ButtonFace icon="pause" label={text.pause} />
        </button>
        <button
          className="preview-toolpaths__button"
          disabled={stepWhy !== undefined}
          onClick={() => void preview.stepMove()}
          data-tip={whyTip(text.stepTitle, stepWhy)}
          type="button"
        >
          <ButtonFace icon="stepForward" label={text.step} />
        </button>
        <button
          className="preview-toolpaths__button"
          disabled={stepWhy !== undefined}
          onClick={() => void preview.runToRetract()}
          data-tip={whyTip(text.toRetractTitle, stepWhy)}
          type="button"
        >
          <ButtonFace icon="moveRetract" label={text.toRetract} />
        </button>
      </div>
      <progress
        aria-label={text.progress}
        className="preview-toolpaths__progress"
        max={1}
        value={preview.animating ? preview.progress : 0}
      />
      <SliderField
        disabled={!preview.animate}
        label={text.speed}
        max={SPEED_MAX}
        min={SPEED_MIN}
        onChange={preview.setSpeed}
        step={1}
        value={preview.speed}
      />
      <button
        className="preview-toolpaths__button"
        disabled={allSidesWhy !== undefined}
        onClick={() => void preview.run(visibleOnSides(toolpaths, activeSide, true))}
        type="button"
        data-tip={whyTip(text.allSides, allSidesWhy)}
      >
        <ButtonFace icon="preview" label={text.allSides} />
      </button>
      <button
        className="preview-toolpaths__button"
        disabled={allWhy !== undefined}
        onClick={() =>
          void preview.run(
            toolpaths.map((t) => t.id),
            true,
          )
        }
        type="button"
        data-tip={whyTip(text.all, allWhy)}
      >
        <ButtonFace icon="preview" label={text.all} />
      </button>
      <button
        className="preview-toolpaths__button preview-toolpaths__button--primary"
        disabled={visibleWhy !== undefined}
        onClick={previewVisible}
        type="button"
        data-tip={whyTip(text.visible, visibleWhy)}
      >
        <ButtonFace icon="visible" label={text.visible} />
      </button>
      <div className="preview-toolpaths__pair">
        <button
          className="preview-toolpaths__button"
          disabled={resetWhy !== undefined}
          onClick={() => void preview.reset()}
          type="button"
          data-tip={whyTip(text.reset, resetWhy)}
        >
          <ButtonFace icon="formReset" label={text.reset} />
        </button>
        <button
          className="preview-toolpaths__button"
          disabled={undoWhy !== undefined}
          onClick={() => void preview.undoLast()}
          data-tip={whyTip(text.undoLastTitle, undoWhy)}
          type="button"
        >
          <ButtonFace icon="editUndo" label={text.undoLast} />
        </button>
      </div>
      <button
        className="preview-toolpaths__button"
        disabled={imageWhy !== undefined}
        onClick={() => void saveShadedImage()}
        type="button"
        data-tip={whyTip(text.saveImage, imageWhy)}
      >
        <ButtonFace icon="saveImage" label={text.saveImage} />
      </button>
      <p className="preview-toolpaths__hint">{text.wasteHint}</p>
      <div aria-live="polite" className="preview-toolpaths__status">
        {preview.running ? <p className="preview-toolpaths__hint">{text.running}</p> : null}
        {preview.heightfield !== null && !preview.running && !preview.animating ? (
          <p className="preview-toolpaths__hint">{text.time(formatDuration(preview.seconds))}</p>
        ) : null}
        {preview.outOfDate ? <p className="preview-toolpaths__notice">{text.outOfDate}</p> : null}
      </div>
      <PreviewChecks key={units} names={names} units={units} />
      <FormButtons onClose={closeForm} />
      {appearance ? (
        <Suspense fallback={null}>
          <MaterialAppearanceDialog
            colour={job.appearance?.colour ?? defaults.solidColour}
            material={job.appearance?.material ?? defaults.material}
            onApply={(draft) =>
              void applyJob({
                ...job,
                appearance: {
                  material: draft.material,
                  colour: draft.colour,
                  texture: draft.texture ?? 0,
                },
              })
            }
            onClose={() => setAppearance(false)}
            texture={job.appearance?.texture ?? 0}
          />
        </Suspense>
      ) : null}
    </div>
  );
}
