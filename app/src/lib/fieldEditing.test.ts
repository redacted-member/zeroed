import { describe, expect, it } from 'vitest';

import { fieldEditFor, isTextField, type FieldInfo } from './fieldEditing';

const field = (over: Partial<FieldInfo>): FieldInfo => ({
  tag: 'INPUT',
  type: 'text',
  readOnly: false,
  disabled: false,
  contentEditable: false,
  ...over,
});

describe('editing keys in text fields', () => {
  it('knows which elements take typed text', () => {
    expect(isTextField(field({}))).toBe(true);
    expect(isTextField(field({ tag: 'TEXTAREA', type: '' }))).toBe(true);
    expect(isTextField(field({ type: 'number' }))).toBe(true);
    expect(isTextField(field({ type: 'checkbox' }))).toBe(false);
    expect(isTextField(field({ tag: 'BUTTON', type: '' }))).toBe(false);
    expect(isTextField(field({ tag: 'DIV', type: '', contentEditable: true }))).toBe(true);
    expect(isTextField(field({ disabled: true }))).toBe(false);
    expect(isTextField(null)).toBe(false);
  });

  it('gives Select All, Undo, Redo and Delete to a focused text box', () => {
    const text = field({ tag: 'TEXTAREA', type: '' });
    expect(fieldEditFor('edit.selectAll', text)).toBe('selectAll');
    expect(fieldEditFor('edit.undo', text)).toBe('undo');
    expect(fieldEditFor('edit.redo', text)).toBe('redo');
    expect(fieldEditFor('edit.delete', text)).toBe('forwardDelete');
    expect(fieldEditFor('file.save', text)).toBeNull();
  });

  it('leaves the commands to the app elsewhere', () => {
    expect(fieldEditFor('edit.selectAll', null)).toBeNull();
    expect(fieldEditFor('edit.undo', field({ type: 'checkbox' }))).toBeNull();
    expect(fieldEditFor('edit.undo', field({ readOnly: true }))).toBeNull();
    expect(fieldEditFor('edit.selectAll', field({ readOnly: true }))).toBe('selectAll');
  });
});
