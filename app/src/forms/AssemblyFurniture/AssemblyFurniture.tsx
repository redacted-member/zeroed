import { useEffect, useId, type JSX } from 'react';

import { furnitureStrings } from '../../lib/strings/furniture';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { useFurnitureStore } from '../../state/furnitureStore';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './AssemblyFurniture.scss';

const text = furnitureStrings.furniture;

/**
 * The Furniture group at the top of the Assembly View form: New Furniture
 * opens the library, and each design in the job has a row that opens it in
 * the Furniture Designer.
 */
export function AssemblyFurniture(): JSX.Element {
  const designs = useFurnitureStore((s) => s.designs);
  const revision = useDocumentStore((s) => s.revision);
  const headingId = useId();

  // The rows follow the document: undo and redo bring designs back or take them.
  useEffect(() => {
    void useFurnitureStore.getState().loadDesigns();
  }, [revision]);

  return (
    <section aria-labelledby={headingId} className="assembly-furniture">
      <h3 className="assembly-furniture__heading" id={headingId}>
        {text.group}
      </h3>
      <button
        className="assembly-furniture__button"
        data-tip={text.newFurnitureTip}
        onClick={() => useDialogStore.getState().show('furnitureLibrary')}
        type="button"
      >
        <ButtonFace icon="furniture" label={text.newFurniture} />
      </button>
      {designs.length === 0 ? null : (
        <ul className="assembly-furniture__list">
          {designs.map((design) => (
            <li key={design.id}>
              <button
                className="assembly-furniture__row"
                data-tip={text.editDesign(design.name)}
                onClick={() => void useFurnitureStore.getState().edit(design.id)}
                type="button"
              >
                <ButtonFace icon="furniture" label={design.name} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
