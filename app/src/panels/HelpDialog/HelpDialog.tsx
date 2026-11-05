import { useEffect, useId, useMemo, useRef, useState, type JSX } from 'react';

import { dialogsStrings } from '../../lib/strings/dialogs';
import { useDialogStore } from '../../state/dialogStore';
import { GUIDE_IDS, useGuideStore } from '../../state/guideStore';
import { GUIDES } from '../Guide/model';
import { GuideIcon } from '../GuideIcon/GuideIcon';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { findTopic, helpSections, type HelpTopic } from './model';

import './HelpDialog.scss';

const text = dialogsStrings.helpDialog;

/** Help > CarveStudio Help: the pages, panels, tools and forms, with a search. */
export function HelpDialog(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'helpContents');
  const serial = useDialogStore((state) => state.helpSerial);
  const close = useDialogStore((state) => state.close);
  return (
    <ModalDialog
      className="help-dialog"
      closeLabel={text.close}
      onClose={close}
      open={open}
      title={text.title}
      titleId="help-dialog-title"
    >
      <HelpBody key={serial} />
    </ModalDialog>
  );
}

function HelpBody(): JSX.Element {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(() => findTopic(useDialogStore.getState().helpTopic).id);
  const sections = useMemo(() => helpSections(query), [query]);
  const shown = useMemo(() => {
    const ids = sections.flatMap((s) => s.groups.flatMap((g) => g.topics.map((t) => t.id)));
    return ids.includes(selected) || ids.length === 0 ? selected : (ids[0] as string);
  }, [sections, selected]);
  const topic = findTopic(shown);
  const searchId = useId();
  const navRef = useRef<HTMLElement | null>(null);
  const articleRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    navRef.current?.querySelector('[aria-current="page"]')?.scrollIntoView?.({ block: 'nearest' });
    articleRef.current?.scrollTo?.({ top: 0 });
  }, [shown]);

  return (
    <div className="help-dialog__layout">
      <nav aria-label={text.contents} className="help-dialog__nav" ref={navRef}>
        <label className="help-dialog__search-label" htmlFor={searchId}>
          {text.search}
        </label>
        <input
          className="help-dialog__search"
          id={searchId}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={text.searchPlaceholder}
          type="search"
          value={query}
        />
        {sections.length === 0 ? <p className="help-dialog__none">{text.noMatch}</p> : null}
        <ul className="help-dialog__toc">
          {sections.map((section) => (
            <li key={section.id}>
              <h3 className="help-dialog__toc-section">{section.title}</h3>
              <ul className="help-dialog__toc">
                {section.groups.map((group) => (
                  <li key={group.id}>
                    {group.title !== '' ? (
                      <h4 className="help-dialog__toc-group">{group.title}</h4>
                    ) : null}
                    <ul className="help-dialog__toc">
                      {group.topics.map((t) => (
                        <li key={t.id}>
                          <button
                            aria-current={t.id === shown ? 'page' : undefined}
                            className="help-dialog__toc-link"
                            onClick={() => setSelected(t.id)}
                            type="button"
                          >
                            {t.title}
                          </button>
                        </li>
                      ))}
                    </ul>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      </nav>
      <article aria-live="polite" className="help-dialog__article" ref={articleRef}>
        <TopicPage onOpen={(id) => setSelected(findTopic(id).id)} topic={topic} />
      </article>
    </div>
  );
}

function TopicPage({
  topic,
  onOpen,
}: {
  topic: HelpTopic;
  onOpen: (id: string) => void;
}): JSX.Element {
  return (
    <>
      <h3 className="help-dialog__title">{topic.title}</h3>
      {topic.summary !== '' ? <p className="help-dialog__summary">{topic.summary}</p> : null}
      {topic.guides ? (
        <ul className="help-dialog__guides">
          {GUIDE_IDS.map((id) => (
            <li key={id}>
              <button
                className="help-dialog__guide"
                data-tip={text.startGuide(GUIDES[id].title)}
                onClick={() => {
                  useDialogStore.getState().close();
                  useGuideStore.getState().start(id);
                }}
                type="button"
              >
                <GuideIcon large name={id} />
                <span className="help-dialog__guide-text">
                  <span className="help-dialog__guide-title">{GUIDES[id].title}</span>
                  <span className="help-dialog__guide-blurb">{GUIDES[id].blurb}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {topic.fields.length > 0 ? (
        <section className="help-dialog__block">
          <h4 className="help-dialog__heading">{text.fields}</h4>
          <dl className="help-dialog__fields">
            {topic.fields.map((field, i) => (
              <div className="help-dialog__field" key={`${field.name}-${i}`}>
                <dt className="help-dialog__field-name">{field.name}</dt>
                <dd className="help-dialog__field-text">{field.text}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : null}
      {topic.steps.length > 0 ? (
        <section className="help-dialog__block">
          <h4 className="help-dialog__heading">{text.steps}</h4>
          <ol className="help-dialog__list">
            {topic.steps.map((step, i) => (
              <li key={i}>{step}</li>
            ))}
          </ol>
        </section>
      ) : null}
      {topic.blocks.map((block) => (
        <section className="help-dialog__block" key={block.heading}>
          <h4 className="help-dialog__heading">{block.heading}</h4>
          <ul className="help-dialog__list">
            {block.items.map((item, i) => (
              <li key={i}>{item}</li>
            ))}
          </ul>
        </section>
      ))}
      {topic.related.length > 0 ? (
        <section className="help-dialog__block">
          <h4 className="help-dialog__heading">{text.seeAlso}</h4>
          <ul className="help-dialog__related">
            {topic.related.map((id) => (
              <li key={id}>
                <button className="help-dialog__link" onClick={() => onOpen(id)} type="button">
                  {findTopic(id).title}
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </>
  );
}
