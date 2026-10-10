import { t } from './i18n.mjs';
/** List definition controls use the existing structural command gate. Reading
 * the caret's definition once per inspector refresh is O(1) in UI controls. */
export function createListAdvanced({ getDoc, getNode, runNodeEdit, onButton, openPanel, ensureGlyphCoverage }) {
  const el = id => document.getElementById(id);
  const start = el('listAdvancedStart'), aligned = el('listAdvancedAligned'), indent = el('listAdvancedIndent'), suffix = el('listAdvancedSuffix');
  function apply(fn) {
    const doc = getDoc(), node = getNode();
    if (!doc || !node) return false;
    const landed = runNodeEdit(() => fn(doc, node));
    reflect();
    return landed;
  }
  onButton(el('listAdvancedSchemeApply'), () => apply((doc, node) => doc.setMultilevelList(node, el('listAdvancedScheme').value)));
  onButton(el('listAdvancedApply'), () => {
    const invalid = [start, aligned, indent].find(input => !input.checkValidity() || !input.value.trim());
    if (invalid) { invalid.reportValidity(); invalid.focus(); return; }
    apply((doc, node) => doc.setListSettings(node, JSON.stringify({ ...JSON.parse(doc.listSettings(node)), start: Number(start.value), alignedAt: Math.round(Number(aligned.value) * 1440), textIndent: Math.round(Number(indent.value) * 1440), suffix: suffix.value })));
  });
  onButton(el('listAdvancedSymbolApply'), () => {
    const input = el('listAdvancedSymbol');
    const symbol = input.value;
    input.setCustomValidity([...symbol].length === 1 && !/\s/u.test(symbol) ? '' : t('listAdvanced.oneSymbol'));
    if (!input.reportValidity()) return;
    if (apply((doc, node) => doc.setListFormat(node, `bullet:${symbol}`))) void ensureGlyphCoverage('list marker');
  });
  onButton(el('listAdvancedPictureApply'), async () => {
    const input = el('listAdvancedPicture'), file = input.files?.[0];
    input.setCustomValidity(!file ? t('listAdvanced.selectPicture') : file.size > 16 * 1024 * 1024 || !['image/png', 'image/jpeg', 'image/webp', 'image/gif'].includes(file.type) ? t('listAdvanced.pictureBounds') : '');
    if (!input.reportValidity()) return;
    const width = el('listAdvancedPictureWidth'), height = el('listAdvancedPictureHeight');
    const invalid = [width, height].find(field => !field.checkValidity() || !field.value.trim());
    if (invalid) { invalid.reportValidity(); invalid.focus(); return; }
    const bytes = new Uint8Array(await file.arrayBuffer());
    apply((doc, node) => doc.setListPictureBullet(node, bytes, Math.round(Number(width.value) * 1440), Math.round(Number(height.value) * 1440), file.type));
  });
  function reflect() {
    const doc = getDoc(), node = getNode();
    const state = doc && node ? JSON.parse(doc.listSettings(node)) : null;
    el('listAdvancedReason').hidden = !!state;
    for (const input of [start, aligned, indent, suffix, el('listAdvancedApply'), el('listAdvancedSchemeApply'), el('listAdvancedSymbolApply'), el('listAdvancedPictureApply')]) input.disabled = !state;
    if (!state) return;
    for (const [input, value] of [[start, state.start ?? 1], [aligned, (state.alignedAt ?? 0) / 1440], [indent, (state.textIndent ?? 720) / 1440], [suffix, state.suffix ?? 'tab']]) if (document.activeElement !== input) input.value = String(value);
    if (state.format?.startsWith('bullet:') && document.activeElement !== el('listAdvancedSymbol')) el('listAdvancedSymbol').value = state.text ?? '•';
  }
  function commands() {
    return [{ id: 'paragraph.list.settings', label: t('listAdvanced.title'), group: 'Paragraph', kw: 'multilevel list start numbering value custom bullet symbol marker aligned indent suffix settings', enabled: !!getNode(), disabledReason: t('paragraph.caretRequired'), run: () => openPanel(() => el('listAdvancedScheme')) }];
  }
  return { reflect, commands };
}
