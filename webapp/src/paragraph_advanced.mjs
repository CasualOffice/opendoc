// RM-20: apply-now inspector using the existing transaction and policy route.
// Readers resolve the caret once per refresh, never once per border/control.
const BORDER_STYLES = new Set(['single','double','dotted','dashed','dotDash','dotDotDash']);
export function borderArguments(edge, color, style, width, padding) {
  const rgb = /^#[0-9a-f]{6}$/i.test(color) ? color.slice(1).match(/../g).map((v) => parseInt(v, 16)) : null;
  const w = Number(width), p = Number(padding);
  if (!BORDER_STYLES.has(style) || !rgb || !Number.isFinite(w) || w < 0.25 || w > 12 || !Number.isInteger(p) || p < 0 || p > 31) return null;
  return [edge, style, ...rgb, Math.round(w * 8), p];
}
export function createParagraphAdvanced({ getDoc, getSelection, getEndpoints, runToolbarEdit, onButton }) {
  const el = (id) => document.getElementById(id);
  const direction = el('paraDirection'), widow = el('paraWidow'), contextual = el('paraContextual'), outline = el('paraOutline');
  async function apply(fn) {
    await runToolbarEdit(fn, { paragraphLevel: true });
    reflect();
  }
  for (const [id, rtl] of [['runDirectionRtl', true], ['runDirectionLtr', false]]) onButton(el(id), async () => { await runToolbarEdit((...range) => getDoc().setTextDirection(...range, rtl)); reflect(); });
  direction.addEventListener('change', () => void apply((...range) => getDoc().setParagraphDirection(...range, direction.value === 'true')));
  for (const [input, setter, value] of [[widow, 'setParagraphWidowControl', () => widow.checked], [contextual, 'setParagraphContextualSpacing', () => contextual.checked], [outline, 'setParagraphOutlineLevel', () => Number(outline.value)]]) input.addEventListener('change', () => void apply((...range) => getDoc()[setter](...range, value())));
  onButton(el('paraClearAll'), () => void apply((...range) => getDoc().clearParagraphFormatting(...range)));
  for (const button of [...el('paragraphPropertiesPanel').querySelectorAll('.border-btn'), el('paraBorderBetween')]) {
    onButton(button, () => {
      const args = borderArguments(button.dataset.border ?? 'between', el('borderColor').value, el('paraBorderStyle').value, el('paraBorderWidth').value, el('paraBorderPadding').value);
      el('paraBorderStyle').setCustomValidity(BORDER_STYLES.has(el('paraBorderStyle').value) ? '' : el('paraBorderImportedNote').textContent);
      if (args) void apply((...range) => getDoc().setParagraphBorderAdvanced(...range, ...args));
      else (!el('paraBorderStyle').checkValidity() ? el('paraBorderStyle') : el('paraBorderWidth').checkValidity() ? el('paraBorderPadding') : el('paraBorderWidth')).reportValidity();
    });
  }
  function reflect() {
    const doc = getDoc(), node = getSelection()?.focus.node;
    if (!doc || !node) return;
    const state = JSON.parse(doc.selectionParagraphAdvanced(...getEndpoints()));
    const range = getEndpoints();
    const hasRange = range[0] !== range[2] || range[1] !== range[3];
    for (const id of ['runDirectionRtl', 'runDirectionLtr']) { el(id).disabled = !hasRange; el(id).title = hasRange ? '' : el('runDirectionReason').textContent; }
    const runDirection = doc.textDirectionState(...range);
    for (const [id, value] of [['runDirectionRtl', 1], ['runDirectionLtr', 0]]) el(id).setAttribute('aria-pressed', runDirection === 2 ? 'mixed' : String(runDirection === value));
    const borders = JSON.parse(doc.paragraphBorders(node));
    el('paraBorderBetween').setAttribute('aria-pressed', String(!!borders.between && !['none','nil'].includes(borders.between.style)));
    const edge = [borders.between, borders.top, borders.bottom, borders.start, borders.end].find((value) => value && !['none','nil'].includes(value.style));
    const style = edge?.style === 'dashSmallGap' ? 'dashed' : edge?.style === 'dashDotStroked' ? 'dotDash' : edge?.style;
    const imported = !!edge && !BORDER_STYLES.has(style);
    el('paraBorderImportedNote').hidden = !imported;
    if (edge) {
      for (const [id, value] of [['paraBorderStyle', imported ? 'imported' : style], ['paraBorderWidth', (edge.sizeEighthPoints ?? 8) / 8], ['paraBorderPadding', edge.spacePoints ?? 0]]) {
        if (document.activeElement !== el(id)) el(id).value = String(value);
      }
    }
    if (document.activeElement !== direction) direction.value = state.rtlState === 2 ? '' : String(state.rtlState === 1);
    for (const [input, value] of [[widow, state.widowState], [contextual, state.contextualState]]) { input.indeterminate = value === 2; input.checked = value === 1; }
    if (document.activeElement !== outline) outline.value = state.outlineMixed ? '' : String(state.outlineLevel);
  }
  function commands() {
    const range = getSelection() ? getEndpoints() : null;
    const hasRange = !!range && (range[0] !== range[2] || range[1] !== range[3]);
    return [['runDirectionRtl', true], ['runDirectionLtr', false]].map(([id, rtl]) => ({
      id: `format.direction.${rtl ? 'rtl' : 'ltr'}`, label: el(id).textContent.trim(), group: 'Format',
      kw: 'selected text direction rtl ltr bidi arabic hebrew', enabled: hasRange, disabledReason: el('runDirectionReason').textContent,
      run: () => el(id).click(),
    }));
  }
  return { reflect, commands };
}
