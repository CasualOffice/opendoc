import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MARGIN_PRESETS, PAPER_PRESETS, matchPreset, mountPagePresets } from '../src/page_presets.mjs';

/** A margin chip: just enough of a button for `bindRadioGroup`. */
function chip(value) {
  const attrs = { 'data-margins': value };
  return {
    disabled: false,
    tabIndex: -1,
    getAttribute: (name) => attrs[name] ?? null,
    setAttribute: (name, v) => { attrs[name] = String(v); },
    removeAttribute: (name) => { delete attrs[name]; },
    closest() { return this; },
    focus() {},
  };
}

function setup(landscape = false) {
  const fields = new Map();
  const chips = ['normal', 'narrow', 'moderate', 'wide', 'custom'].map(chip);
  const group = {
    listeners: {},
    querySelectorAll: () => chips,
    contains: () => true,
    addEventListener(event, fn) { this.listeners[event] = fn; },
  };
  const el = (id) => {
    if (id === 'pageMarginsPreset') return group;
    if (!fields.has(id)) fields.set(id, { value: '0', hidden: true, listeners: {}, addEventListener(event, fn) { this.listeners[event] = fn; }, focus() {} });
    return fields.get(id);
  };
  let previews = 0;
  const api = mountPagePresets({ el, measure: { format: (n) => String(n / 1440), read: (n) => Math.round(Number(n) * 1440) }, orientation: () => (landscape ? 'landscape' : 'portrait'), updatePreview: () => previews++ });
  const choose = (id, value) => { el(id).value = value; el(id).listeners.change(); };
  const pick = (value) => group.listeners.click({ target: chips.find((c) => c.getAttribute('data-margins') === value), preventDefault() {} });
  const checked = () => chips.find((c) => c.getAttribute('aria-checked') === 'true')?.getAttribute('data-margins') ?? null;
  return { el, api, choose, pick, checked, previews: () => previews };
}

test('paper presets populate geometry in orientation and reader units without writing a document', () => {
  const form = setup(true);
  form.choose('pagePaperPreset', 'legal');
  assert.equal(form.el('pageWidth').value, '14');
  assert.equal(form.el('pageHeight').value, '8.5');
  assert.equal(form.previews(), 1);
  assert.equal(form.el('pageCustomSize').hidden, true, 'a named size hides the width and height fields');
  form.api.reflect();
  assert.equal(form.el('pagePaperPreset').value, 'legal');
  form.el('pageWidth').value = '13';
  form.api.reflect();
  assert.equal(form.el('pagePaperPreset').value, 'custom');
  assert.equal(form.el('pageCustomSize').hidden, false, 'a custom size shows its fields');
});

test('margin chips fill the four edges and leave the gutter alone', () => {
  const form = setup();
  form.el('pageMarginGutter').value = '0.25';
  form.pick('moderate');
  assert.deepEqual(['Top', 'Bottom', 'Left', 'Right'].map((edge) => form.el(`pageMargin${edge}`).value), ['1', '1', '0.75', '0.75']);
  assert.equal(form.el('pageMarginGutter').value, '0.25');
  form.api.reflect();
  assert.equal(form.checked(), 'moderate');
});

// Audit DIALOGS-10: an A4 page said "Custom". The fields carry two decimals of
// the reader's unit, so A4's 11906 x 16838 twips reads back as 8.27 x 11.69 in
// = 11909 x 16834. Mutation: PRESET_TOLERANCE_TWIPS = 1 turns this red.
test('an A4 page read back through two-decimal inches is still A4', () => {
  const read = (twips) => Math.round(Number((twips / 1440).toFixed(2)) * 1440);
  assert.equal(matchPreset(PAPER_PRESETS, PAPER_PRESETS.a4.map(read)), 'a4');
  assert.equal(matchPreset(PAPER_PRESETS, PAPER_PRESETS.letter.map(read)), 'letter');
  assert.equal(matchPreset(MARGIN_PRESETS, [1440, 1440, 1080, 1080]), 'moderate');
  // ...and a millimetre is the limit, not a licence: a page 5 mm off is custom.
  assert.equal(matchPreset(PAPER_PRESETS, [11906 + 283, 16838]), 'custom');
});
