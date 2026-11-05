import { useRef, useState, type JSX, type KeyboardEvent, type MouseEvent } from 'react';

import type { Sheet, SheetMaterial } from '../../bridge/document';
import { menuAnchor } from '../../lib/menuAnchor';
import { strings } from '../../lib/strings';
import { whyStrings } from '../../lib/strings/why';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { confirmationOn } from '../../state/prefsStore';
import { IconButton } from '../../forms/IconButton/IconButton';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import { RenameField } from '../RenameField/RenameField';
import { SheetEditor } from '../SheetEditor/SheetEditor';
import {
  addSources,
  buildSheetMenu,
  canDelete,
  deleteWhy,
  deletePrompt,
  livePicks,
  nextSheetName,
  pickSheets,
  sheetSize,
  stepForKey,
  stepSheet,
  type SheetAction,
} from './model';

import './SheetsPanel.scss';

const text = strings.sheets;

/** The Sheets tab: every sheet of the job, which one is active, and the
 * controls to add, edit, rename, duplicate and delete them. */
export function SheetsPanel(): JSX.Element {
  const sheets = useDocumentStore((store) => store.sheets);
  const units = useDocumentStore((store) => store.job.units);
  const selectedVectors = useDocumentStore((store) => store.selection.ids.length);
  const selectedComponent = useModelStore((store) => store.selectedId);
  const store = useDocumentStore.getState;

  const [picked, setPicked] = useState<number[]>([]);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [renaming, setRenaming] = useState<number | null>(null);
  const [editing, setEditing] = useState<number[] | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: number } | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);

  const live = livePicks(sheets, picked);
  const selectedCount = selectedVectors + (selectedComponent === null ? 0 : 1);

  const focusRow = (id: number) =>
    listRef.current?.querySelector<HTMLLIElement>(`[data-sheet="${id}"]`)?.focus();

  const pick = (id: number, event: MouseEvent | KeyboardEvent) => {
    const toggle = event.metaKey || event.ctrlKey;
    setPicked(pickSheets(sheets, live, anchor, id, { toggle, range: event.shiftKey }));
    if (!event.shiftKey) {
      setAnchor(id);
    }
  };

  const activate = async (id: number) => {
    if (sheets.find((s) => s.active)?.id !== id) {
      await store().setActiveSheet(id);
    }
    focusRow(id);
  };

  const addNew = async (sources: readonly Sheet[]) => {
    const taken: string[] = [];
    for (const source of sources) {
      const name = nextSheetName(store().sheets, taken);
      taken.push(name);
      await store().addSheetLike(name, source);
    }
  };

  const remove = async (ids: readonly number[]) => {
    if (!canDelete(sheets, ids)) {
      return;
    }
    const doomed = sheets.filter((s) => ids.includes(s.id));
    const question = deletePrompt(doomed);
    if (question !== null && confirmationOn('deleteSheet') && !window.confirm(question)) {
      return;
    }
    for (const sheet of doomed) {
      await store().deleteSheet(sheet.id);
    }
    setPicked([]);
  };

  const moveHere = (id: number) =>
    void store().moveSelectionToSheet(
      id,
      selectedComponent === null ? undefined : [selectedComponent],
    );

  const onMenuAction = (action: SheetAction, sheet: Sheet) => {
    // Menu actions work on the picked sheets when the clicked one is among them.
    const targets = live.includes(sheet.id) ? live : [sheet.id];
    switch (action) {
      case 'edit':
        setEditing(targets);
        break;
      case 'rename':
        setRenaming(sheet.id);
        break;
      case 'makeActive':
        void activate(sheet.id);
        break;
      case 'duplicate':
        void store().duplicateSheet(sheet.id);
        break;
      case 'moveHere':
        moveHere(sheet.id);
        break;
      case 'delete':
        void remove(targets);
        break;
      case 'newSheet':
        void addNew(addSources(sheets, targets));
        break;
    }
  };

  const onRowKeyDown = (event: KeyboardEvent<HTMLLIElement>, sheet: Sheet) => {
    if (event.target !== event.currentTarget) {
      return;
    }
    const step = stepForKey(event.key);
    if (step !== null) {
      event.preventDefault();
      const target = stepSheet(sheets, sheet.id, step);
      if (target !== null) {
        pick(target, event);
        focusRow(target);
      }
    } else if (event.key === 'Enter') {
      event.preventDefault();
      void activate(sheet.id);
    } else if (event.key === ' ') {
      event.preventDefault();
      pick(sheet.id, event);
    } else if (event.key === 'F2') {
      event.preventDefault();
      setRenaming(sheet.id);
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      void remove(live.length > 0 ? live : [sheet.id]);
    }
  };

  const editSheets = editing === null ? [] : sheets.filter((s) => editing.includes(s.id));
  const editMaterial: SheetMaterial | undefined = editSheets[0]?.material;
  if (editing !== null && editMaterial !== undefined) {
    return (
      <div className="sheets-panel">
        <SheetEditor
          material={editMaterial}
          onApply={(material, scaleDesign) => {
            void store().editSheets(
              editSheets.map((s) => s.id),
              material,
              scaleDesign,
            );
            setEditing(null);
          }}
          onClose={() => setEditing(null)}
          sheets={editSheets}
          units={units}
        />
      </div>
    );
  }

  const menuSheet = menu === null ? null : (sheets.find((s) => s.id === menu.id) ?? null);
  const focusId = live[0] ?? sheets.find((s) => s.active)?.id ?? sheets[0]?.id;

  return (
    <div className="sheets-panel">
      <ul
        aria-label={text.listLabel}
        aria-multiselectable="true"
        className="sheets-panel__list"
        ref={listRef}
        role="listbox"
      >
        {sheets.map((sheet) => {
          const isPicked = live.includes(sheet.id);
          return (
            <li
              aria-selected={isPicked}
              className={['sheets-panel__row', isPicked ? 'sheets-panel__row--picked' : '']
                .join(' ')
                .trim()}
              data-sheet={sheet.id}
              key={sheet.id}
              onClick={(event) => pick(sheet.id, event)}
              onContextMenu={(event) => {
                event.preventDefault();
                if (!isPicked) {
                  setPicked([sheet.id]);
                  setAnchor(sheet.id);
                }
                setMenu({ ...menuAnchor(event), id: sheet.id });
              }}
              onDoubleClick={() => void activate(sheet.id)}
              onKeyDown={(event) => onRowKeyDown(event, sheet)}
              role="option"
              tabIndex={sheet.id === focusId ? 0 : -1}
              data-tip={text.rowTitle(sheet.name)}
            >
              {renaming === sheet.id ? (
                <RenameField
                  label={text.renameField(sheet.name)}
                  name={sheet.name}
                  onDone={(name) => {
                    setRenaming(null);
                    if (name !== null) {
                      void store().renameSheet(sheet.id, name);
                    }
                    focusRow(sheet.id);
                  }}
                />
              ) : (
                <>
                  <span
                    aria-current={sheet.active ? 'true' : undefined}
                    className="sheets-panel__name"
                  >
                    {sheet.name}
                    {sheet.active ? (
                      <span className="sheets-panel__sr-only">{`, ${text.active}`}</span>
                    ) : null}
                  </span>
                  <span className="sheets-panel__detail">{sheetSize(sheet, units)}</span>
                  <span className="sheets-panel__detail">
                    {text.contents(sheet.count ?? 0, sheet.components ?? 0)}
                  </span>
                </>
              )}
            </li>
          );
        })}
      </ul>
      <div className="sheets-panel__buttons">
        <IconButton
          description={text.addNewHint}
          icon="sheetAdd"
          label={text.addNew}
          onClick={() => void addNew(addSources(sheets, live))}
          title={text.addNewTitle}
        />
        <IconButton
          description={text.editHint}
          why={live.length === 0 ? whyStrings.why.pickSheet : undefined}
          icon="sheetEdit"
          label={text.editButton}
          onClick={() => setEditing(live)}
          title={text.editTitle}
        />
        <IconButton
          description={sheets.length <= 1 ? text.keepOne : text.deleteHint}
          why={deleteWhy(sheets, live)}
          icon="sheetDelete"
          label={text.deleteButton}
          onClick={() => void remove(live)}
          title={text.deleteTitle}
        />
      </div>
      {menu === null || menuSheet === null ? null : (
        <PopupMenu
          entries={buildSheetMenu({ sheet: menuSheet, sheetCount: sheets.length, selectedCount })}
          label={text.menuLabel(menuSheet.name)}
          onClose={() => setMenu(null)}
          onSelect={(action) => onMenuAction(action, menuSheet)}
          x={menu.x}
          y={menu.y}
        />
      )}
    </div>
  );
}
