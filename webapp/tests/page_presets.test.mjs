import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mountPagePresets } from '../src/page_presets.mjs';
function setup(landscape = false) {
  const fields = new Map();
  const el = id => { if (!fields.has(id)) fields.set(id, { value: '0', listeners: {}, addEventListener(event, fn) { this.listeners[event] = fn; } }); return fields.get(id); };
  let previews = 0;
  const api = mountPagePresets({ el, measure: { format: n => String(n / 1440), read: n => Math.round(Number(n) * 1440) }, orientation: () => landscape ? 'landscape' : 'portrait', updatePreview: () => previews++ });
  const choose = (id, value) => { el(id).value = value; el(id).listeners.change(); };
  return { el, api, choose, previews: () => previews };
}
test('paper presets populate geometry in orientation and reader units without writing a document', () => {
  const form = setup(true);
  form.choose('pagePaperPreset', 'legal');
  assert.equal(form.el('pageWidth').value, '14');
  assert.equal(form.el('pageHeight').value, '8.5');
  assert.equal(form.previews(), 1);
  form.api.reflect();
  assert.equal(form.el('pagePaperPreset').value, 'legal');
  form.el('pageWidth').value = '13';
  form.api.reflect();
  assert.equal(form.el('pagePaperPreset').value, 'custom');
});
test('margin presets preserve gutter and independent header/footer fields', () => {
  const form = setup();
  form.el('pageMarginGutter').value = '0.25';
  form.choose('pageMarginsPreset', 'moderate');
  assert.deepEqual(['Top', 'Bottom', 'Left', 'Right'].map(edge => form.el(`pageMargin${edge}`).value), ['1', '1', '0.75', '0.75']);
  assert.equal(form.el('pageMarginGutter').value, '0.25');
  form.api.reflect();
  assert.equal(form.el('pageMarginsPreset').value, 'moderate');
});
