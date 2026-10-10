import { t } from './i18n.mjs';
import { styleDisplayName } from './style_names.mjs';
/** Registry edits reuse the toolbar transaction/policy route. Definition reads
 * are bounded by the style registry, performed when this inspector is shown. */
export function createStyleAdvanced({ getDoc, hasCaret, currentStyle, runToolbarEdit, onButton, openPanel, refresh }) {
  const el = id => document.getElementById(id);
  const target = el('styleAdvancedTarget'), next = el('styleAdvancedNext');
  function options(select, names, blank = false) {
    select.replaceChildren();
    for (const name of blank ? ['', ...names] : names) {
      const option = document.createElement('option');
      option.value = name; option.textContent = name ? styleDisplayName(name) : t('styleAdvanced.keep');
      select.appendChild(option);
    }
  }
  function reflectNext() { next.value = getDoc()?.styleNext(target.value) ?? ''; }
  function reflect() {
    const doc = getDoc(), names = doc?.listStyles() ?? [];
    const wanted = target.value || currentStyle();
    options(target, names); options(next, names, true);
    target.value = names.includes(wanted) ? wanted : names[0] ?? '';
    reflectNext();
    for (const id of ['styleAdvancedTarget', 'styleAdvancedNext', 'styleAdvancedNextApply', 'styleAdvancedDelete', 'styleAdvancedRestore']) el(id).disabled = !names.length || !hasCaret();
  }
  async function apply(method, ...args) {
    if (!getDoc() || !target.value) return;
    await runToolbarEdit(() => getDoc()[method](target.value, ...args));
    refresh(); reflect();
  }
  target.addEventListener('change', reflectNext);
  onButton(el('styleAdvancedNextApply'), () => void apply('setStyleNext', next.value));
  onButton(el('styleAdvancedDelete'), () => void apply('deleteStyle'));
  onButton(el('styleAdvancedRestore'), () => void apply('restoreStyleDefault'));
  return { reflect, commands: () => [{ id: 'style.manage', label: t('styleAdvanced.title'), group: 'Style', kw: 'delete style following paragraph next restore saved definition', enabled: hasCaret(), disabledReason: t('paragraph.caretRequired'), run: () => openPanel(() => target) }] };
}
