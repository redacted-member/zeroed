import { useEffect, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { KernelError } from '../../bridge/types';
import type {
  AttachmentRow,
  CutListRow,
  FurnitureEstimate,
  HardwareRow,
} from '../../bridge/furniture';
import { useFurnitureStore } from '../../state/furnitureStore';
import type { Units } from '../../bridge/types';
import { formatLength } from '../../lib/units';
import { furnitureStrings } from '../../lib/strings/furniture';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { OptionTabs } from '../OptionTabs/OptionTabs';
import {
  areaText,
  carvingCount,
  cutListCsv,
  estimateCsv,
  hardwareCsv,
  panelName,
  rowName,
  timeText,
} from '../FurnitureDesigner/model';
import { exportCsv } from './exportCsv';

import './FurnitureLists.scss';

const text = furnitureStrings.furniture.designer;
const faceText = furnitureStrings.furniture.face;

function carvingText(count: number): string {
  return count === 0 ? faceText.noCarving : faceText.models(count);
}

interface Lists {
  cutList: CutListRow[];
  hardware: HardwareRow[];
  estimate: FurnitureEstimate;
  attachments: AttachmentRow[];
}

interface FurnitureListsProps {
  /** The design in the document, or null before Create. */
  design: number | null;
  name: string;
  units: Units;
  /** Changes whenever the design is made again, so the lists reload. */
  revision: number;
  onNotice: (notice: string) => void;
  onError: (error: string) => void;
}

/** Cut List, Hardware and Estimate tabs of a made design, each with Export CSV. */
export function FurnitureLists({
  design,
  name,
  units,
  revision,
  onNotice,
  onError,
}: FurnitureListsProps): JSX.Element {
  const [lists, setLists] = useState<Lists | null>(null);

  useEffect(() => {
    if (design === null) {
      return undefined;
    }
    let live = true;
    void Promise.all([
      api.furniture.cutList(design),
      api.furniture.hardware(design),
      api.furniture.estimate(design),
      // Models on faces come with a later kernel; the lists show without them.
      api.furniture.attachments(design).catch(() => ({ attachments: [] as AttachmentRow[] })),
    ])
      .then(([cut, hardware, estimate, attached]) => {
        if (live) {
          setLists({
            cutList: cut.rows,
            hardware: hardware.rows,
            estimate,
            attachments: attached.attachments,
          });
        }
      })
      .catch((error: unknown) => {
        // A kernel without the lists yet: the tabs stay on their hint.
        if (error instanceof KernelError && error.code === 'unknown_command') {
          return;
        }
        onError(error instanceof Error ? error.message : String(error));
      });
    return () => {
      live = false;
    };
  }, [design, onError, revision]);

  const save = (csv: string, what: string) => {
    void exportCsv(csv, `${name} ${what}.csv`)
      .then((notice) => {
        if (notice !== null) {
          onNotice(notice);
        }
      })
      .catch((error: unknown) => onError(error instanceof Error ? error.message : String(error)));
  };

  if (design === null || lists === null) {
    return (
      <section className="furniture-lists">
        <p className="furniture-lists__hint">{text.listsAfterCreate}</p>
      </section>
    );
  }

  const exportButton = (csv: string, what: string) => (
    <button className="furniture-lists__button" onClick={() => save(csv, what)} type="button">
      <ButtonFace icon="exportFile" label={text.exportCsv} />
    </button>
  );
  const length = (mm: number) => formatLength(mm, units);
  const setStretch = (row: AttachmentRow, stretch: boolean) => {
    void api.furniture
      .setStretch(design, row.kind, row.id, stretch)
      .then(() => {
        setLists({
          ...lists,
          attachments: lists.attachments.map((a) =>
            a.kind === row.kind && a.id === row.id ? { ...a, stretch } : a,
          ),
        });
      })
      .catch((error: unknown) => onError(error instanceof Error ? error.message : String(error)));
  };

  return (
    <section className="furniture-lists">
      <OptionTabs
        label={text.tabs}
        tabs={[
          {
            id: 'cutList',
            label: text.cutList,
            content: (
              <>
                <table className="furniture-lists__table">
                  <thead>
                    <tr>
                      {[0, 2, 3, 4, 5].map((i) => (
                        <th key={i} scope="col">
                          {text.cutListHeader[i]}
                        </th>
                      ))}
                      <th scope="col">{faceText.carving}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {lists.cutList.map((row) => (
                      <tr key={row.panels.join(' ')}>
                        <th scope="row">{rowName(row)}</th>
                        <td>{row.quantity}</td>
                        <td>{length(row.length)}</td>
                        <td>{length(row.width)}</td>
                        <td>{length(row.thickness)}</td>
                        <td>{carvingText(carvingCount(row, lists.attachments))}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {lists.attachments.length > 0 && (
                  <table className="furniture-lists__table">
                    <caption className="furniture-lists__caption">{faceText.onFaces}</caption>
                    <thead>
                      <tr>
                        {faceText.onFacesHeader.map((h) => (
                          <th key={h} scope="col">
                            {h}
                          </th>
                        ))}
                        <th scope="col">{faceText.stretch}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {lists.attachments.map((a) => (
                        <tr key={`${a.kind}-${a.id}`}>
                          <th scope="row">{panelName(lists.cutList, a.panel)}</th>
                          <td>{a.faceWord}</td>
                          <td>{faceText.kinds[a.kind]}</td>
                          <td>
                            <input
                              aria-label={`${faceText.stretch}, ${panelName(lists.cutList, a.panel)}`}
                              checked={a.stretch}
                              className="furniture-lists__check"
                              data-tip={faceText.stretchTip}
                              onChange={(event) => setStretch(a, event.target.checked)}
                              type="checkbox"
                            />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                <div className="furniture-lists__buttons">
                  {exportButton(cutListCsv(lists.cutList, units), text.cutList)}
                  <button
                    className="furniture-lists__button"
                    data-tip={faceText.makeCarvingTip}
                    onClick={() => void useFurnitureStore.getState().makeCarving()}
                    type="button"
                  >
                    <ButtonFace icon="finish3d" label={faceText.makeCarving} />
                  </button>
                </div>
              </>
            ),
          },
          {
            id: 'hardware',
            label: text.hardware,
            content: (
              <>
                {lists.hardware.length === 0 ? (
                  <p className="furniture-lists__hint">{text.noHardware}</p>
                ) : (
                  <table className="furniture-lists__table">
                    <thead>
                      <tr>
                        {text.hardwareHeader.map((h) => (
                          <th key={h} scope="col">
                            {h}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {lists.hardware.map((row) => (
                        <tr key={row.name}>
                          <th scope="row">{row.name}</th>
                          <td>{row.count}</td>
                          <td>{row.note}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                {exportButton(hardwareCsv(lists.hardware), text.hardware)}
              </>
            ),
          },
          {
            id: 'estimate',
            label: text.estimate,
            content: (
              <>
                <table className="furniture-lists__table">
                  <thead>
                    <tr>
                      {text.estimateHeader.map((h) => (
                        <th key={h} scope="col">
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {lists.estimate.sheets.map((m) => (
                      <tr key={m.material}>
                        <th scope="row">{m.label}</th>
                        <td>{length(m.thickness)}</td>
                        <td>{m.count}</td>
                        <td>{areaText(m.used, units)}</td>
                        <td>{text.percent(m.waste)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <p className="furniture-lists__hint">
                  {text.cutTime(timeText(lists.estimate.seconds))}
                </p>
                {exportButton(estimateCsv(lists.estimate, units), text.estimate)}
              </>
            ),
          },
        ]}
      />
    </section>
  );
}
