import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createColumnEditor} from '../src/page_columns.mjs';
function setup(previous) {
  const nodes = new Map();
  const el = id => {
    if (!nodes.has(id)) nodes.set(id, {value: '0', checked: false, disabled: false, listeners: {}, label: {hidden: false}, closest() {return this.label;}, addEventListener(kind, fn) {this.listeners[kind] = fn;}});
    return nodes.get(id);
  };
  el('count').value = String(previous.count); el('gap').value = String(previous.spaceTwips);
  const editor = createColumnEditor({el, measure: {read: Number, format: String}, count: el('count'), gap: el('gap'), separator: el('separator'), contentWidth: () => 9000});
  editor.reflect(previous);
  return {el, editor};
}
test('unrelated page edits preserve exact imported unequal widths and per-column spacing', () => {
  const previous = {count: 2, spaceTwips: 720, separator: false, equalWidth: false, columns: [{widthTwips: 3000, spaceTwips: 900}, {widthTwips: 5000}], imported: 'retained'};
  const {el, editor} = setup(previous);
  assert.equal(editor.payload(previous), previous);
  assert.equal(el('pageColumnWidth0').disabled, false);
  assert.equal(el('pageColumnSpace1').disabled, true);
  el('pageColumnWidth0').value = '3200';
  const edited = editor.payload(previous);
  assert.equal(edited.equalWidth, false); assert.equal(edited.columns[0].widthTwips, 3200);
  assert.equal(edited.columns[0].spaceTwips, 900); assert.equal(edited.columns[1].widthTwips, 5000);
  assert.equal(edited.imported, 'retained');
});
