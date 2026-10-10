// A form adapter: presets fill the existing fields; Apply remains the single
// transactional writer. Values are twips, independent of the reader's units.
export const PAPER_PRESETS = Object.freeze({ letter: [12240, 15840], legal: [12240, 20160], a4: [11906, 16838], a5: [8391, 11906] });
export const MARGIN_PRESETS = Object.freeze({ normal: [1440, 1440, 1440, 1440], narrow: [720, 720, 720, 720], moderate: [1440, 1440, 1080, 1080], wide: [1440, 1440, 2880, 2880] });

export function mountPagePresets({ el, measure, orientation, updatePreview }) {
  const paper = el('pagePaperPreset');
  const margins = el('pageMarginsPreset');
  const dimensions = ['pageWidth', 'pageHeight'].map(el);
  const edges = ['pageMarginTop', 'pageMarginBottom', 'pageMarginLeft', 'pageMarginRight'].map(el);
  const read = fields => fields.map(field => measure.read(field.value));
  const match = (presets, values) => Object.entries(presets).find(([, candidate]) => candidate.every((value, index) => Math.abs(value - values[index]) <= 1))?.[0] ?? 'custom';
  const oriented = values => orientation() === 'landscape' ? [...values].reverse() : values;
  function reflect() {
    paper.value = match(Object.fromEntries(Object.entries(PAPER_PRESETS).map(([key, value]) => [key, oriented(value)])), read(dimensions));
    margins.value = match(MARGIN_PRESETS, read(edges));
  }
  function fill(fields, values) {
    fields.forEach((field, index) => { field.value = measure.format(values[index]); });
    updatePreview();
  }
  paper.addEventListener('change', () => { const preset = PAPER_PRESETS[paper.value]; if (preset) fill(dimensions, oriented(preset)); });
  margins.addEventListener('change', () => { const preset = MARGIN_PRESETS[margins.value]; if (preset) fill(edges, preset); });
  for (const field of [...dimensions, ...edges]) field.addEventListener('input', reflect);
  return { reflect };
}
