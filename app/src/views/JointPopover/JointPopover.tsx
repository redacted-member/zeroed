import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type JSX } from 'react';

import type { ConnectionRow, PreviewPanel } from '../../bridge/furniture';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { galleryItems, jointText } from '../../forms/FurnitureDesigner/jointItems';
import { JointGallery } from '../../forms/JointGallery/JointGallery';
import { furnitureStrings } from '../../lib/strings/furniture';
import { useFurnitureStore } from '../../state/furnitureStore';
import { SeamView3D } from '../SeamView3D/SeamView3D';
import { midpoint } from '../View3D/furnitureSeams';
import { popoverPlace } from './model';

import './JointPopover.scss';

const text = furnitureStrings.furniture.seam;

interface JointPopoverProps {
  seam: ConnectionRow;
  /** Where the click was, CSS px in the view. */
  x: number;
  y: number;
  /** The view's size, CSS px. */
  viewWidth: number;
  viewHeight: number;
  /** The class's label, for Use for all like this. */
  classLabel: string;
  panels: readonly PreviewPanel[];
}

/**
 * The joints a clicked seam can take as an icon grid, the current one
 * picked, Use for all like this (the seam's whole class), and the two
 * panels drawn small. Each pick is one undo step; Esc closes it.
 */
export function JointPopover({
  seam,
  x,
  y,
  viewWidth,
  viewHeight,
  classLabel,
  panels,
}: JointPopoverProps): JSX.Element {
  const [forAll, setForAll] = useState(false);
  const ref = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ width: 300, height: 360 });
  const titleId = useId();
  const checkId = useId();
  const items = useMemo(() => galleryItems(seam.choices), [seam.choices]);
  const pair = useMemo(
    () => panels.filter((p) => p.key === seam.a || p.key === seam.b),
    [panels, seam.a, seam.b],
  );
  const close = () => useFurnitureStore.getState().setPopover(null);
  const nameOf = (key: string) => panels.find((p) => p.key === key)?.name ?? key;

  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) {
      return;
    }
    const rect = element.getBoundingClientRect();
    if (Math.abs(rect.width - size.width) > 1 || Math.abs(rect.height - size.height) > 1) {
      setSize({ width: rect.width, height: rect.height });
    }
  }, [size.height, size.width, pair.length]);

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>('[aria-checked="true"]')?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        useFurnitureStore.getState().setPopover(null);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [seam.key]);

  const place = popoverPlace(x, y, size.width, size.height, viewWidth, viewHeight);

  return (
    <div
      aria-labelledby={titleId}
      className="joint-popover"
      ref={ref}
      role="dialog"
      style={{ transform: `translate(${place.left}px, ${place.top}px)` }}
    >
      <h3 className="joint-popover__title" id={titleId}>
        {text.title(nameOf(seam.a), nameOf(seam.b))}
      </h3>
      <JointGallery
        items={items}
        label={text.joints}
        onChange={(joint) =>
          void useFurnitureStore.getState().pickSeamJoint(seam.key, joint, forAll)
        }
        value={seam.joint}
      />
      <div className="joint-popover__check">
        <input
          checked={forAll}
          id={checkId}
          onChange={(event) => setForAll(event.target.checked)}
          type="checkbox"
        />
        <label htmlFor={checkId} data-tip={text.forAllHint(classLabel)}>
          {text.forAll}
        </label>
      </div>
      {pair.length === 0 ? null : (
        <SeamView3D
          focus={midpoint(seam.seam.from, seam.seam.to)}
          label={jointText(seam.joint).name}
          panels={pair}
        />
      )}
      <button className="joint-popover__button" onClick={close} type="button">
        <ButtonFace icon="dismiss" label={text.close} />
      </button>
    </div>
  );
}
