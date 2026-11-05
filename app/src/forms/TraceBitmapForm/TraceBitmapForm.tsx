import { useEffect, useId, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { TraceFit, TracePalette } from '../../bridge/api';
import type { Bbox, Path } from '../../bridge/document';
import { fadeBitmaps } from '../../lib/bitmapCommands';
import { numberValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { usePreview, useDrawController } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { useTraceViewStore } from '../../state/traceViewStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  colourHex,
  cornerFitFromSmoothness,
  smoothnessFromCornerFit,
  fadeOf,
  initialColourState,
  MAX_COLOURS,
  MIN_COLOURS,
  pickColour,
  reducedPixels,
  regionFromDrag,
  regionPaths,
  resetLinked,
  toggleLinked,
  tracedColours,
  withColourCount,
  type ColourTraceState,
  initialTraceState,
  opacityOf,
  selectedBitmap,
  traceErrors,
  traceParamsOf,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './TraceBitmapForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { tracePicture } from '../HelperPicture/pictures/drawing';

const text = strings.createVectors.traceBitmap;

const FITS: ReadonlyArray<Choice<TraceFit>> = [
  { value: 'lines', label: text.lines },
  { value: 'curves', label: text.curves },
];

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Smallest drag, in screen pixels at the current zoom, that sets a trace area. */
const AREA_MIN_MM = 0.5;

/**
 * While a placed bitmap is traced, clicks on the picture pick the trace
 * colour (and clear the trace area) and drags set the trace area.
 */
function TraceAreaController(props: {
  onClick: (x: number, y: number) => void;
  onArea: (region: Bbox | null) => void;
  onDrag: (region: Bbox | null) => void;
}): null {
  const { onClick, onArea, onDrag } = props;
  const from = useRef<{ x: number; y: number } | null>(null);
  const controller = useMemo<DrawController>(
    () => ({
      cursor: 'crosshair',
      pointerDown: (p) => {
        from.current = { x: p.rawX, y: p.rawY };
      },
      pointerMove: (p) => {
        if (from.current !== null) {
          onDrag(regionFromDrag(from.current, { x: p.rawX, y: p.rawY }, AREA_MIN_MM));
        }
      },
      pointerUp: (p) => {
        const start = from.current;
        if (start === null) {
          return;
        }
        const region = regionFromDrag(start, { x: p.rawX, y: p.rawY }, AREA_MIN_MM);
        from.current = null;
        onDrag(null);
        if (region === null) {
          onArea(null);
          onClick(p.rawX, p.rawY);
        } else {
          onArea(region);
        }
      },
      cancel: () => {
        from.current = null;
        onDrag(null);
      },
    }),
    [onClick, onArea, onDrag],
  );
  useDrawController(controller);
  return null;
}

export function TraceBitmapForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState(initialTraceState);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // A selected bitmap is traced where it lies; it stays the source while
  // the traced vectors take over the selection.
  const picked = useDocumentStore((store) => selectedBitmap(store.entities, store.selection.ids));
  const exists = useDocumentStore((store) => store.entities);
  const [chosen, setChosen] = useState<number | null>(picked);
  const [lastPicked, setLastPicked] = useState<number | null>(picked);
  if (picked !== lastPicked) {
    setLastPicked(picked);
    if (picked !== null) {
      setChosen(picked);
    }
  }
  const source = chosen !== null && exists.some((e) => e.id === chosen) ? chosen : null;
  const [fade, setFade] = useState(0);
  const [imageId, setImageId] = useState<number | null>(null);
  const fadeId = useId();
  const fitId = useId();
  const [traced, setTraced] = useState<{ key: string; paths: Path[] } | null>(null);
  const countId = useId();
  useEffect(() => {
    if (source === null) {
      return;
    }
    let live = true;
    void api.entities
      .get(source)
      .then((entity) => {
        if (live && entity.kind === 'bitmap') {
          setFade(fadeOf(entity.opacity));
          setImageId(entity.imageId);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [source]);
  const onBitmap = source !== null;
  const applyFade = () => {
    if (source !== null) {
      void fadeBitmaps([source], opacityOf(fade));
    }
  };

  const [colours, setColours] = useState<ColourTraceState>(initialColourState);
  const [fetched, setFetched] = useState<{ key: string; palette: TracePalette } | null>(null);
  const paletteKey = `${String(source)}:${String(colours.count)}`;
  const palette = fetched !== null && fetched.key === paletteKey ? fetched.palette : null;
  const [region, setRegion] = useState<Bbox | null>(null);
  const [dragged, setDragged] = useState<Bbox | null>(null);
  useEffect(() => {
    if (source === null) {
      return;
    }
    let live = true;
    const key = `${String(source)}:${String(colours.count)}`;
    void api.bitmap
      .tracePalette(source, colours.count)
      .then((reply) => {
        if (live) {
          setFetched({ key, palette: reply });
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [source, colours.count]);
  const colourMode = onBitmap && palette !== null && palette.colour;
  const [fetchedIndices, setFetchedIndices] = useState<{
    key: string;
    width: number;
    height: number;
    indices: Uint8Array;
  } | null>(null);
  useEffect(() => {
    if (source === null || !colourMode) {
      return;
    }
    let live = true;
    const key = `${String(source)}:${String(colours.count)}`;
    void api.bitmap
      .traceIndices(source, colours.count)
      .then((reply) => {
        if (live) {
          setFetchedIndices({ key, ...reply });
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [source, colourMode, colours.count]);
  // The picture shows in its reduced colours while the form traces by colour.
  useEffect(() => {
    const view = useTraceViewStore.getState();
    const indices = fetchedIndices?.key === paletteKey ? fetchedIndices : null;
    if (!colourMode || imageId === null || palette === null || indices === null) {
      view.setOverride(null);
      return;
    }
    let live = true;
    const pixels = reducedPixels(
      indices.indices,
      palette.colours.map((colour) => colour.rgb),
      colours,
    );
    void createImageBitmap(new ImageData(pixels, indices.width, indices.height))
      .then((image) => {
        if (live) {
          view.setOverride({ imageId, image });
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [colourMode, imageId, palette, fetchedIndices, paletteKey, colours]);
  useEffect(() => () => useTraceViewStore.getState().setOverride(null), []);
  const onViewClick = useMemo(
    () => (x: number, y: number) => {
      if (source === null || !colourMode) {
        return;
      }
      void api.bitmap
        .colourAt(source, colours.count, x, y)
        .then(({ index }) => {
          if (index !== null) {
            setColours((current) => pickColour(current, index));
          }
        })
        .catch(() => undefined);
    },
    [source, colourMode, colours.count],
  );

  const errors = useMemo(() => traceErrors(state), [state]);
  const params = traceParamsOf(state);
  const previewKey = JSON.stringify([source, params, colours, region]);
  const preview = useMemo(
    () => [
      ...regionPaths(dragged ?? region),
      // Preview's outlines, while nothing that changes them has moved.
      ...(traced !== null && traced.key === previewKey ? traced.paths : []),
    ],
    [dragged, region, traced, previewKey],
  );
  usePreview(preview);

  const tracedList = tracedColours(colours);
  const resetWhy = colours.linked.length === 0 ? strings.why.noLinks : undefined;
  const ready = onBitmap && params !== null && (!colourMode || tracedList.length > 0);

  const runPreview = () => {
    if (!ready || busy || source === null || params === null) {
      return;
    }
    setFailure(null);
    const key = previewKey;
    api.bitmap
      .tracePreview(source, params, {
        ...(colourMode ? { colours: { count: colours.count, trace: tracedList } } : {}),
        ...(region !== null ? { region } : {}),
      })
      .then((reply) => {
        setTraced({ key, paths: reply.paths });
        setStatus(text.previewed(reply.paths.length));
      })
      .catch((error: unknown) => setFailure(message(error)));
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!ready || busy || source === null || params === null) {
      return;
    }
    setBusy(true);
    setFailure(null);
    setStatus(null);
    api.bitmap
      .traceImage(source, params, {
        ...(colourMode ? { colours: { count: colours.count, trace: tracedList } } : {}),
        ...(region !== null ? { region } : {}),
      })
      .then(async (reply) => {
        await useDocumentStore.getState().select(reply.ids);
        setStatus(text.created(reply.ids.length));
      })
      .catch(async (error: unknown) => {
        setFailure(message(error));
        await useDocumentStore.getState().refresh();
      })
      .finally(() => setBusy(false));
  };

  return (
    <form className="trace-bitmap" onSubmit={onSubmit}>
      <p className="trace-bitmap__hint">{onBitmap ? text.bitmapHint : text.hint}</p>
      {onBitmap ? (
        <TraceAreaController onArea={setRegion} onClick={onViewClick} onDrag={setDragged} />
      ) : null}
      {colourMode && palette !== null ? (
        <fieldset className="trace-bitmap__group">
          <legend className="trace-bitmap__legend">{text.colours}</legend>
          <label className="trace-bitmap__hint" htmlFor={countId}>
            {text.colourCount(colours.count)}
          </label>
          <input
            className="trace-bitmap__range"
            id={countId}
            max={MAX_COLOURS}
            min={MIN_COLOURS}
            onChange={(event) =>
              setColours((current) => withColourCount(current, Number(event.target.value)))
            }
            step={1}
            type="range"
            value={colours.count}
          />
          <p className="trace-bitmap__hint">{text.colourHint}</p>
          <ul className="trace-bitmap__colours">
            {palette.colours.map((colour, index) => {
              const linked = colours.linked.includes(index);
              const active = colours.trace === index;
              return (
                <li
                  className={
                    active || linked
                      ? 'trace-bitmap__colour trace-bitmap__colour--traced'
                      : 'trace-bitmap__colour'
                  }
                  key={colourHex(colour.rgb) + String(index)}
                >
                  <span
                    aria-hidden="true"
                    className="trace-bitmap__swatch"
                    style={{ backgroundColor: colourHex(colour.rgb) }}
                  />
                  <label className="trace-bitmap__pick">
                    <input
                      aria-label={text.colourName(index, colour.pixels)}
                      checked={active}
                      name="traceColour"
                      onChange={() => setColours((current) => pickColour(current, index))}
                      type="radio"
                    />
                    {text.traceColour}
                  </label>
                  <label className="trace-bitmap__pick">
                    <input
                      aria-label={`${text.link} ${text.colourName(index, colour.pixels)}`}
                      checked={linked}
                      onChange={() => setColours((current) => toggleLinked(current, index))}
                      type="checkbox"
                    />
                    {text.link}
                  </label>
                </li>
              );
            })}
          </ul>
          <button
            className="trace-bitmap__choose"
            disabled={resetWhy !== undefined}
            onClick={() => setColours(resetLinked)}
            type="button"
            data-tip={whyTip(text.resetLinks, resetWhy)}
          >
            <ButtonFace icon="formReset" label={text.resetLinks} />
          </button>
          {tracedList.length === 0 ? <p className="trace-bitmap__hint">{text.pickFirst}</p> : null}
        </fieldset>
      ) : null}
      {onBitmap ? (
        <fieldset className="trace-bitmap__group">
          <legend className="trace-bitmap__legend">{text.area}</legend>
          <p className="trace-bitmap__hint">{text.areaHint}</p>
          <p className="trace-bitmap__hint" role="status">
            {region === null ? text.areaAll : text.areaSet}
          </p>
        </fieldset>
      ) : null}
      <fieldset className="trace-bitmap__group">
        <legend className="trace-bitmap__legend">{text.settings}</legend>
        {colourMode ? null : (
          <>
            <NumberField
              error={errors.threshold}
              label={text.threshold}
              onChange={(threshold) => setState((current) => ({ ...current, threshold }))}
              step={1}
              value={state.threshold}
            />
            <p className="trace-bitmap__hint">{text.thresholdHint}</p>
          </>
        )}
        <NumberField
          error={errors.noise}
          label={text.noise}
          onChange={(noise) => setState((current) => ({ ...current, noise }))}
          step={1}
          unit={text.pixelUnit}
          value={state.noise}
        />
        <HelperRow picture={<HelperPicture picture={tracePicture(state.fit)} />}>
          <ChoiceField
            label={text.fit}
            onChange={(fit) => setState((current) => ({ ...current, fit }))}
            options={FITS}
            value={state.fit}
          />
        </HelperRow>
        <label className="trace-bitmap__hint" htmlFor={fitId}>
          {text.cornerFitSlider}
        </label>
        <div className="trace-bitmap__slider">
          <span className="trace-bitmap__end">{text.loose}</span>
          <input
            className="trace-bitmap__range"
            id={fitId}
            max={100}
            min={0}
            onChange={(event) =>
              setState((current) => ({
                ...current,
                smoothness: String(smoothnessFromCornerFit(Number(event.target.value))),
              }))
            }
            step={5}
            type="range"
            value={cornerFitFromSmoothness(numberValue(state.smoothness))}
          />
          <span className="trace-bitmap__end">{text.tight}</span>
        </div>
      </fieldset>
      {onBitmap ? (
        <fieldset className="trace-bitmap__group">
          <legend className="trace-bitmap__legend">{text.fading}</legend>
          <label className="trace-bitmap__hint" htmlFor={fadeId}>
            {text.fadingLabel(fade)}
          </label>
          <input
            className="trace-bitmap__range"
            id={fadeId}
            max={100}
            min={0}
            onChange={(event) => setFade(Number(event.target.value))}
            onKeyUp={applyFade}
            onPointerUp={applyFade}
            step={5}
            type="range"
            value={fade}
          />
        </fieldset>
      ) : null}
      {failure !== null ? (
        <p className="trace-bitmap__error" role="alert">
          {failure}
        </p>
      ) : null}
      {status !== null ? (
        <p className="trace-bitmap__hint" role="status">
          {status}
        </p>
      ) : null}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        onSecondary={onBitmap ? runPreview : undefined}
        primaryWhy={firstWhy(
          [busy, strings.why.busy],
          [!onBitmap, strings.why.pickBitmap],
          [params === null, strings.why.fixValues],
          [!ready, text.pickFirst],
        )}
        primaryLabel={text.action}
        secondaryLabel={onBitmap ? text.preview : undefined}
        submit
      />
    </form>
  );
}
