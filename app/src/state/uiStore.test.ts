import { beforeEach, describe, expect, it } from 'vitest';

import { TOOL_GROUPS } from '../shell/LeftPanel/toolGroups';
import { formDock } from './formDock';
import { useUiStore } from './uiStore';

function reset() {
  useUiStore.setState({
    activeTab: 'design',
    activeForm: null,
    editingToolpath: null,
    toolpathsPinned: true,
    toolpathsOpen: false,
    toolpathsFocus: 0,
    leftPinned: true,
    leftOpen: false,
    leftWidth: 268,
    rightWidth: 300,
  });
}

describe('form docks', () => {
  it('opens every toolpath tool in the right dock and every other tool on the left', () => {
    for (const [set, groups] of Object.entries(TOOL_GROUPS)) {
      for (const tool of groups.flatMap((group) => group.tools)) {
        if (tool.form !== undefined) {
          expect([tool.id, formDock(tool.form)]).toEqual([
            tool.id,
            set === 'toolpaths' ? 'right' : 'left',
          ]);
        }
      }
    }
    expect(formDock('mergeToolpaths')).toBe('right');
    expect(formDock('gadget')).toBe('left');
    expect(formDock('snapSettings')).toBe('left');
  });
});

describe('uiStore', () => {
  beforeEach(reset);

  it('opens a toolpath form in the right dock without leaving the left tab', () => {
    useUiStore.setState({ activeTab: 'layers', toolpathsPinned: false });
    useUiStore.getState().openForm('profileToolpath');
    const ui = useUiStore.getState();
    expect(ui.activeForm).toBe('profileToolpath');
    expect(ui.activeTab).toBe('layers');
    expect(ui.toolpathsOpen).toBe(true);
  });

  it('edits a toolpath in the right dock', () => {
    useUiStore.getState().editToolpath('pocketToolpath', 7);
    const ui = useUiStore.getState();
    expect(ui.activeForm).toBe('pocketToolpath');
    expect(ui.editingToolpath).toBe(7);
    expect(ui.activeTab).toBe('design');
  });

  it('shows a left form on the Design tab when a list tab is showing', () => {
    useUiStore.setState({ activeTab: 'clipart' });
    useUiStore.getState().openForm('sculpt');
    expect(useUiStore.getState().activeTab).toBe('design');
    useUiStore.setState({ activeTab: 'layers', activeForm: null });
    useUiStore.getState().openForm('move');
    expect(useUiStore.getState().activeTab).toBe('design');
  });

  it('keeps a component form on the Components tab it was opened from', () => {
    useUiStore.setState({ activeTab: 'components' });
    useUiStore.getState().openForm('componentProperties');
    expect(useUiStore.getState().activeTab).toBe('components');
    useUiStore.setState({ activeForm: null });
    useUiStore.getState().openForm('move');
    expect(useUiStore.getState().activeTab).toBe('design');
  });

  it('closes a left form when the tab changes, but keeps a toolpath form', () => {
    useUiStore.getState().openForm('move');
    useUiStore.getState().setActiveTab('layers');
    expect(useUiStore.getState().activeForm).toBeNull();
    useUiStore.getState().openForm('drillToolpath');
    useUiStore.getState().setActiveTab('design');
    expect(useUiStore.getState().activeForm).toBe('drillToolpath');
  });

  it('keeps a left form when the same tab is chosen again', () => {
    useUiStore.getState().openForm('offset');
    useUiStore.getState().setActiveTab('design');
    expect(useUiStore.getState().activeForm).toBe('offset');
  });

  it('pins, unpins and shows the Toolpaths panel', () => {
    const ui = useUiStore.getState();
    ui.toggleToolpathsPinned();
    expect(useUiStore.getState().toolpathsPinned).toBe(false);
    expect(useUiStore.getState().toolpathsOpen).toBe(false);
    useUiStore.getState().showToolpaths();
    expect(useUiStore.getState().toolpathsOpen).toBe(true);
    expect(useUiStore.getState().toolpathsFocus).toBe(1);
  });

  it('pins and unpins the left tool panel and keeps dock widths in range', () => {
    const ui = useUiStore.getState();
    ui.toggleLeftPinned();
    expect(useUiStore.getState().leftPinned).toBe(false);
    ui.setLeftOpen(true);
    ui.toggleLeftPinned();
    expect(useUiStore.getState()).toMatchObject({ leftPinned: true, leftOpen: false });
    ui.setDockWidth('left', 999);
    ui.setDockWidth('right', 12.6);
    expect(useUiStore.getState()).toMatchObject({ leftWidth: 640, rightWidth: 200 });
    ui.setDockWidth('right', 333.4);
    expect(useUiStore.getState().rightWidth).toBe(333);
  });

  it('switches to the machining layout and back to the design layout (F12, F11)', () => {
    useUiStore.setState({ activeTab: 'layers', toolpathsPinned: false, leftPinned: true });
    useUiStore.getState().openForm('offset');
    const focus = useUiStore.getState().toolpathsFocus;
    useUiStore.getState().machiningLayout();
    expect(useUiStore.getState()).toMatchObject({
      leftPinned: false,
      toolpathsPinned: true,
      toolpathsFocus: focus + 1,
      activeForm: 'offset',
    });
    useUiStore.getState().designLayout();
    expect(useUiStore.getState()).toMatchObject({
      activeTab: 'design',
      activeForm: 'offset',
      leftPinned: true,
      toolpathsPinned: false,
    });
    useUiStore.getState().setActiveTab('layers');
    useUiStore.getState().designLayout();
    expect(useUiStore.getState().activeTab).toBe('design');
  });

  it('shows the unpinned left panel when a left form opens', () => {
    useUiStore.setState({ leftPinned: false, leftOpen: false });
    useUiStore.getState().openForm('offset');
    expect(useUiStore.getState().leftOpen).toBe(true);
    useUiStore.getState().closeForm();
    expect(useUiStore.getState().leftOpen).toBe(false);
  });
});

describe('Job Setup for a new job', () => {
  beforeEach(reset);

  it('brings the 2D view up in place of the 3D view, where the material is drawn', () => {
    useUiStore.setState({ viewMode: '3d' });
    useUiStore.getState().openNewJobSetup();
    expect(useUiStore.getState().activeForm).toBe('jobSetup');
    expect(useUiStore.getState().viewMode).toBe('2d');
  });

  it('keeps a split layout, which shows the 2D view already', () => {
    useUiStore.setState({ viewMode: 'split' });
    useUiStore.getState().openNewJobSetup();
    expect(useUiStore.getState().viewMode).toBe('split');
  });
});
