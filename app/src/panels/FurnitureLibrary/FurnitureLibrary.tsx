import { useEffect, useId, useMemo, useState, type JSX } from 'react';

import { getKernel } from '../../bridge/client';
import { api } from '../../bridge/formApi';
import type { TemplateSummary } from '../../bridge/furniture';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { initialJoints } from '../../forms/FurnitureDesigner/model';
import { furnitureStrings } from '../../lib/strings/furniture';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { useFurnitureStore } from '../../state/furnitureStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import {
  ALL,
  categoriesOf,
  defaultValues,
  filterTemplates,
  licenseBadge,
  needsCredit,
} from './model';

import './FurnitureLibrary.scss';

const text = furnitureStrings.furniture;
const THUMB_SIZE = 160;

/** The name a category shows: the strings' one, or the template's own word. */
function categoryLabel(id: string): string {
  if (id === ALL) {
    return text.library.all;
  }
  return (text.categories as Readonly<Record<string, string>>)[id] ?? id;
}

/**
 * Draws the thumbnails of these templates one at a time, each from the
 * kernel's preview at its defaults, cached per template id in the store.
 */
function useThumbnails(templates: readonly TemplateSummary[], open: boolean): void {
  const thumbnails = useFurnitureStore((s) => s.thumbnails);
  const waiting = templates.find((t) => !(t.id in thumbnails));
  useEffect(() => {
    if (!open || waiting === undefined) {
      return undefined;
    }
    let live = true;
    void (async () => {
      let url: string | null = null;
      try {
        const [preview, meshes] = await Promise.all([
          api.furniture.preview(
            { template: waiting.id },
            defaultValues(waiting.params, useDocumentStore.getState().job.units),
            initialJoints(),
          ),
          import('../../views/View3D/furnitureMeshes'),
        ]);
        url = meshes.renderThumbnail(preview.panels, THUMB_SIZE);
      } catch {
        url = null;
      }
      if (live) {
        useFurnitureStore.getState().setThumbnail(waiting.id, url);
      }
    })();
    return () => {
      live = false;
    };
  }, [open, waiting]);
}

/**
 * The Furniture Library: categories down the side, a search, and a card
 * per template with its picture at the default size, one line, license and
 * credit. Picking a card opens the Furniture Designer on it.
 */
export function FurnitureLibrary(): JSX.Element {
  const open = useDialogStore((s) => s.open === 'furnitureLibrary');
  const close = useDialogStore((s) => s.close);
  const templates = useFurnitureStore((s) => s.templates);
  const loading = useFurnitureStore((s) => s.loadingTemplates);
  const error = useFurnitureStore((s) => s.error);
  const [category, setCategory] = useState<string>(ALL);
  const [query, setQuery] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const titleId = useId();
  const searchId = useId();

  useEffect(() => {
    if (open) {
      void useFurnitureStore.getState().loadTemplates();
    }
  }, [open]);

  const all = useMemo(() => templates ?? [], [templates]);
  const shown = useMemo(() => filterTemplates(all, category, query), [all, category, query]);
  const categories = useMemo(() => categoriesOf(all), [all]);
  useThumbnails(shown, open);

  const pick = (template: TemplateSummary) => {
    close();
    void useFurnitureStore.getState().startNew(template.id);
  };

  const importTemplate = async () => {
    const kernel = getKernel();
    const path = await kernel.chooseOpenPath(text.library.importTitle, [
      { name: text.library.templateFiles, extensions: ['json'] },
    ]);
    if (path === null) {
      return;
    }
    try {
      const { text: json } = await api.io.readText(path);
      if (await useFurnitureStore.getState().importTemplate(json)) {
        setNotice(text.library.imported(path));
      }
    } catch (failure) {
      useFurnitureStore
        .getState()
        .setError(failure instanceof Error ? failure.message : String(failure));
    }
  };

  const openFolder = async () => {
    const kernel = getKernel();
    if (kernel.openDataFolder === undefined) {
      setNotice(text.library.noFolder);
      return;
    }
    await kernel.openDataFolder('furniture');
  };

  const removeTemplate = async (template: TemplateSummary) => {
    if (await getKernel().confirm(text.library.confirmRemove(template.name))) {
      await useFurnitureStore.getState().removeTemplate(template.id);
    }
  };

  return (
    <ModalDialog
      className="furniture-library"
      closeLabel={text.designer.close}
      footer={
        <>
          <button
            className="furniture-library__button"
            data-tip={text.library.importTip}
            onClick={() => void importTemplate()}
            type="button"
          >
            <ButtonFace icon="fileOpen" label={text.library.importTemplate} />
          </button>
          <button
            className="furniture-library__button"
            data-tip={text.library.openFolderTip}
            onClick={() => void openFolder()}
            type="button"
          >
            <ButtonFace icon="folder" label={text.library.openFolder} />
          </button>
        </>
      }
      onClose={close}
      open={open}
      title={text.library.title}
      titleId={titleId}
    >
      <div className="furniture-library__body">
        <nav aria-label={text.library.categories} className="furniture-library__categories">
          <ul className="furniture-library__category-list">
            {categories.map((c) => (
              <li key={c.id === ALL ? 'all' : c.id}>
                <button
                  aria-pressed={category === c.id}
                  className="furniture-library__category"
                  onClick={() => setCategory(c.id)}
                  type="button"
                >
                  <span>{categoryLabel(c.id)}</span>
                  <span className="furniture-library__count">{c.count}</span>
                </button>
              </li>
            ))}
          </ul>
        </nav>
        <div className="furniture-library__main">
          <div className="furniture-library__search">
            <label className="furniture-library__label" htmlFor={searchId}>
              {text.library.search}
            </label>
            <input
              className="furniture-library__input"
              id={searchId}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={text.library.searchPlaceholder}
              type="search"
              value={query}
            />
          </div>
          {loading && templates === null ? (
            <p className="furniture-library__hint">{text.library.loading}</p>
          ) : shown.length === 0 ? (
            <p className="furniture-library__hint">{text.library.empty}</p>
          ) : (
            <ul className="furniture-library__grid">
              {shown.map((t) => (
                <TemplateCard
                  key={t.id}
                  onPick={() => pick(t)}
                  onRemove={t.builtIn ? null : () => void removeTemplate(t)}
                  template={t}
                />
              ))}
            </ul>
          )}
          {error === null ? null : (
            <p className="furniture-library__error" role="alert">
              {error}
            </p>
          )}
          {notice === null ? null : (
            <p aria-live="polite" className="furniture-library__hint">
              {notice}
            </p>
          )}
        </div>
      </div>
    </ModalDialog>
  );
}

function TemplateCard({
  template,
  onPick,
  onRemove,
}: {
  template: TemplateSummary;
  onPick: () => void;
  onRemove: (() => void) | null;
}): JSX.Element {
  const thumbnail = useFurnitureStore((s) => s.thumbnails[template.id]);
  const badge = licenseBadge(template.license);
  return (
    <li className="furniture-library__card">
      <button
        aria-label={text.library.use(template.name)}
        className="furniture-library__pick"
        onClick={onPick}
        type="button"
      >
        <span className="furniture-library__thumb">
          {thumbnail === undefined ? null : thumbnail === null ? (
            <span className="furniture-library__no-thumb">{text.library.thumbFailed}</span>
          ) : (
            <img
              alt={text.library.thumbnail(template.name)}
              className="furniture-library__image"
              height={THUMB_SIZE}
              src={thumbnail}
              width={THUMB_SIZE}
            />
          )}
        </span>
        <span className="furniture-library__name">{template.name}</span>
        <span className="furniture-library__line">{template.description}</span>
      </button>
      <span className="furniture-library__meta">
        {badge === null ? null : (
          <span className="furniture-library__badge" data-tip={text.licenses[`${badge}Tip`]}>
            {text.licenses[badge]}
          </span>
        )}
        <span className="furniture-library__author">
          {template.builtIn ? text.library.by(template.author) : text.library.mine}
        </span>
        {onRemove === null ? null : (
          <button
            aria-label={text.library.removeTemplate}
            className="furniture-library__remove"
            data-tip={text.library.removeTip}
            onClick={onRemove}
            type="button"
          >
            <ButtonFace icon="itemRemove" />
          </button>
        )}
      </span>
      {needsCredit(template.license) && template.credit !== null ? (
        <span className="furniture-library__credit">{template.credit}</span>
      ) : null}
    </li>
  );
}
