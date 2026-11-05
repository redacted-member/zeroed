import { describe, expect, it } from 'vitest';

import { START_TOPIC, TOOLPATHS_TOPIC, contextTopic, dockTopic } from './helpTopics';

describe('help topics', () => {
  it('opens the form shown in a dock, else the dock page', () => {
    expect(dockTopic('left', 'drawCircle', 'design')).toBe('form:drawCircle');
    expect(dockTopic('left', null, 'layers')).toBe('panel:layers');
    // A toolpath form is in the right dock, so the left dock shows its tab.
    expect(dockTopic('left', 'profileToolpath', 'design')).toBe('panel:design');
    expect(dockTopic('right', 'profileToolpath', 'design')).toBe('form:profileToolpath');
    expect(dockTopic('right', 'drawCircle', 'design')).toBe(TOOLPATHS_TOPIC);
  });

  it('opens the open form, else the start page, from the Help menu and F1', () => {
    expect(contextTopic('saveToolpaths')).toBe('form:saveToolpaths');
    expect(contextTopic(null)).toBe(START_TOPIC);
  });
});
