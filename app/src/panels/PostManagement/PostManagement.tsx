import { useId, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { PostChangeLog, PostSummary, PostText } from '../../bridge/cam';
import { getKernel } from '../../bridge/client';
import { reloadPosts, usePosts } from '../../lib/camData';
import { machineSetupStrings } from '../../lib/strings/machineSetup';
import { useDialogStore } from '../../state/dialogStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { features, isCustom, lastChanged, visiblePosts } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './PostManagement.scss';

const text = machineSetupStrings.postManager;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Machine > Post-Processor Management: the posts this machine can write with. */
export function PostManagement(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'postManagement');
  const close = useDialogStore((state) => state.close);
  return (
    <ModalDialog
      className="post-management"
      closeLabel={text.close}
      onClose={close}
      open={open}
      title={text.title}
      titleId="post-management-title"
    >
      <PostBody />
    </ModalDialog>
  );
}

function PostBody(): JSX.Element {
  const posts = usePosts();
  const [filter, setFilter] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [contents, setContents] = useState<PostText | null>(null);
  const [log, setLog] = useState<PostChangeLog | null>(null);
  const [showLog, setShowLog] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const ids = {
    filter: useId(),
    list: useId(),
    details: useId(),
    contents: useId(),
    log: useId(),
  };
  const shown = visiblePosts(posts ?? [], filter);
  const post = (posts ?? []).find((p) => p.id === selected) ?? null;

  const run = async (action: () => Promise<string | null>) => {
    setFailure(null);
    setStatus(null);
    try {
      setStatus(await action());
    } catch (error) {
      setFailure(message(error));
    }
    await reloadPosts();
  };

  const install = () =>
    run(async () => {
      const path = await getKernel().chooseOpenPath(text.install, [
        { name: text.install, extensions: ['pp'] },
      ]);
      if (path === null) {
        return null;
      }
      const reply = await api.posts.import(path);
      pick(reply.id);
      return text.installed(reply.id);
    });

  const pick = (id: string) => {
    setSelected(id);
    setContents(null);
    setShowLog(false);
    setLog(null);
    api.posts
      .changelog(id)
      .then(setLog)
      .catch(() => setLog(null));
  };

  const view = (target: PostSummary) =>
    run(async () => {
      setShowLog(false);
      setContents(await api.posts.text(target.id));
      return null;
    });

  const changeLog = (target: PostSummary) =>
    run(async () => {
      setContents(null);
      setLog(await api.posts.changelog(target.id));
      setShowLog(true);
      return null;
    });

  const editFile = async (target: PostSummary) => {
    const kernel = getKernel();
    if (kernel.editPost === undefined) {
      setStatus(text.noEditor);
      return;
    }
    await run(async () => {
      await kernel.editPost?.(target.id);
      return text.opened(target.id);
    });
  };

  const customize = (target: PostSummary) =>
    run(async () => {
      const reply = await api.posts.customize(target.id);
      pick(reply.id);
      return text.customized(reply.id);
    });

  const remove = async (target: PostSummary) => {
    if (!(await getKernel().confirm(text.confirmRemove(target.name)))) {
      return;
    }
    await run(async () => {
      await api.posts.remove(target.id);
      setSelected(null);
      setContents(null);
      return text.removed(target.name);
    });
  };

  const openFolder = async () => {
    const kernel = getKernel();
    if (kernel.openDataFolder === undefined) {
      setStatus(text.noFolder);
      return;
    }
    await kernel.openDataFolder('posts');
  };

  const copy = async () => {
    if (contents === null) {
      return;
    }
    try {
      await navigator.clipboard.writeText(contents.text);
      setStatus(text.copied);
    } catch (error) {
      setFailure(message(error));
    }
  };

  return (
    <div className="post-management__layout">
      <div className="post-management__top">
        <label className="post-management__label" htmlFor={ids.filter}>
          {text.filter}
        </label>
        <input
          className="post-management__input"
          id={ids.filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={text.filterPlaceholder}
          type="search"
          value={filter}
        />
        <button
          className="post-management__button"
          onClick={() => void install()}
          type="button"
          data-tip={text.install}
        >
          <ButtonFace icon="machinePackage" label={text.install} />
        </button>
      </div>
      <div className="post-management__panes">
        <section aria-labelledby={ids.list} className="post-management__list-pane">
          <h3 className="post-management__heading" id={ids.list}>
            {text.list}
          </h3>
          {shown.length === 0 ? (
            <p className="post-management__hint">{text.none}</p>
          ) : (
            <table className="post-management__table">
              <thead>
                <tr>
                  <th scope="col">{text.columns.name}</th>
                  <th scope="col">{text.columns.extension}</th>
                  <th scope="col">{text.columns.units}</th>
                  <th scope="col">{text.columns.features}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((row) => (
                  <tr
                    aria-selected={row.id === selected}
                    className="post-management__row"
                    key={row.id}
                  >
                    <td>
                      <button
                        aria-pressed={row.id === selected}
                        className="post-management__pick"
                        onClick={() => pick(row.id)}
                        type="button"
                      >
                        {row.name}
                        {isCustom(row) ? (
                          <span className="post-management__badge" data-tip={text.customHint}>
                            {text.custom}
                          </span>
                        ) : null}
                      </button>
                    </td>
                    <td>{row.fileExtension}</td>
                    <td>{row.units}</td>
                    <td>
                      {features(row)
                        .map((f) => text[f])
                        .join(', ')}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
        <section aria-labelledby={ids.details} className="post-management__details">
          <h3 className="post-management__heading" id={ids.details}>
            {text.details}
          </h3>
          {post === null ? null : (
            <>
              <dl className="post-management__facts">
                <dt>{text.columns.name}</dt>
                <dd>{post.name}</dd>
                <dt>{text.identity}</dt>
                <dd>{post.id}</dd>
                <dt>{text.columns.extension}</dt>
                <dd>{post.fileExtension}</dd>
                <dt>{text.columns.units}</dt>
                <dd>{post.units}</dd>
                <dt>{text.source}</dt>
                <dd>{isCustom(post) ? post.sourcePath : text.shipped}</dd>
                {log !== null && log.id === post.id && lastChanged(log.entries) !== null ? (
                  <>
                    <dt>{text.lastChanged}</dt>
                    <dd>{lastChanged(log.entries)}</dd>
                  </>
                ) : null}
              </dl>
              <div className="post-management__buttons">
                <button
                  className="post-management__button"
                  onClick={() => void view(post)}
                  type="button"
                  data-tip={text.view}
                >
                  <ButtonFace icon="notes" label={text.view} />
                </button>
                <button
                  className="post-management__button"
                  onClick={() => void changeLog(post)}
                  type="button"
                  data-tip={text.changeLog}
                >
                  <ButtonFace icon="notes" label={text.changeLog} />
                </button>
                <button
                  className="post-management__button"
                  onClick={() => void customize(post)}
                  type="button"
                  data-tip={text.customize}
                >
                  <ButtonFace icon="options" label={text.customize} />
                </button>
                {isCustom(post) ? (
                  <>
                    <button
                      className="post-management__button"
                      onClick={() => void editFile(post)}
                      type="button"
                      data-tip={text.edit}
                    >
                      <ButtonFace icon="itemEdit" label={text.edit} />
                    </button>
                    <button
                      className="post-management__button"
                      onClick={() => void openFolder()}
                      type="button"
                      data-tip={text.openFolder}
                    >
                      <ButtonFace icon="folder" label={text.openFolder} />
                    </button>
                    <button
                      className="post-management__button"
                      onClick={() => void remove(post)}
                      type="button"
                      data-tip={text.remove}
                    >
                      <ButtonFace icon="itemRemove" label={text.remove} />
                    </button>
                  </>
                ) : null}
              </div>
            </>
          )}
          {showLog && log !== null && post !== null && log.id === post.id ? (
            <section aria-labelledby={ids.log} className="post-management__contents">
              <h4 className="post-management__subheading" id={ids.log}>
                {text.changeLogOf(post.name)}
              </h4>
              {log.entries.length === 0 ? (
                <p className="post-management__hint">{text.noChangeLog}</p>
              ) : (
                <table className="post-management__table post-management__log">
                  <thead>
                    <tr>
                      <th scope="col">{text.changeColumns.who}</th>
                      <th scope="col">{text.changeColumns.when}</th>
                      <th scope="col">{text.changeColumns.what}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {log.entries.map((entry, index) => (
                      <tr key={`${index}-${entry.when}`}>
                        <td>{entry.who}</td>
                        <td>{entry.when}</td>
                        <td>{entry.what}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </section>
          ) : null}
          {contents === null ? null : (
            <section aria-labelledby={ids.contents} className="post-management__contents">
              <div className="post-management__contents-bar">
                <h4 className="post-management__subheading" id={ids.contents}>
                  {text.contents(contents.id)}
                </h4>
                <button
                  className="post-management__button"
                  onClick={() => void copy()}
                  type="button"
                  data-tip={text.copy}
                >
                  <ButtonFace icon="editCopy" label={text.copy} />
                </button>
              </div>
              <pre className="post-management__text">{contents.text}</pre>
            </section>
          )}
          {status === null ? null : (
            <p aria-live="polite" className="post-management__status">
              {status}
            </p>
          )}
          {failure === null ? null : (
            <p className="post-management__error" role="alert">
              {failure}
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
