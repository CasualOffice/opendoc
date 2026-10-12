// RM-18: character authoring owns its UI; the engine owns properties and history.
import { t } from './i18n.mjs';
import { borderArguments } from './paragraph_advanced.mjs';
export const LIGATURE_MODES = Object.freeze(['none','standard','contextual','standardContextual','historical','standardHistorical','contextualHistorical','standardContextualHistorical','discretional','standardDiscretional','contextualDiscretional','standardContextualDiscretional','historicalDiscretional','standardHistoricalDiscretional','contextualHistoricalDiscretional','all']);
const UNITS = Object.freeze({characterSpacingTwips:20, characterScalePercent:1, positionHalfPoints:2, kerningHalfPoints:2});
export function characterNumber(key, value) {
  const number = Number(value), factor = UNITS[key];
  if (value === '') return null;
  if (!factor || !Number.isFinite(number) || Math.abs(number * factor - Math.round(number * factor)) > 1e-7) throw new RangeError(key);
  const result = Math.round(number * factor);
  const [min,max] = key === 'characterScalePercent' ? [1,600] : key === 'kerningHalfPoints' ? [0,65534] : [-31680,31680];
  if (result < min || result > max) throw new RangeError(key);
  return result;
}
export function typeArmedCharacter(doc, focus, text, pf, session, mode) {
  return doc.typeCharacterText(focus.node, focus.offset, text, pf.bold, pf.italic, pf.underline, pf.strike, pf.sizeHalfPoints, pf.color, pf.highlight, pf.vertAlign, pf.font, session, pf.underlineStyle, pf.underlineColor, JSON.stringify(pf.character), mode === 'suggesting', new Date().toISOString());
}
export function applyCharacterPatch(io, character) {
  if (io.blockMutationInViewing()) return;
  if (io.hasRange()) return io.runToolbarEdit((...range) => io.getReviewMode() === 'suggesting'
    ? io.getDoc().suggestCharacterFormat(...range,JSON.stringify(character),new Date().toISOString())
    : io.getDoc().setCharacterFormat(...range,JSON.stringify(character)), {allowInSuggesting:true});
  const pending=io.getPendingFormat() ?? {};
  io.setPendingFormat({...pending,character:{...pending.character,...character}});
  io.updateToolbar();
}
export function createFontAdvanced(io) {
  const getPending = () => io.getPendingFormat()?.character ?? {};
  const canEdit = () => io.getReviewMode() !== 'viewing';
  const apply = patch => applyCharacterPatch(io,patch);
  const el = id => document.getElementById(id), form = el('fontAdvancedForm');
  const inputs = [...form.querySelectorAll('[data-character]')], ligatures = [...form.querySelectorAll('[data-ligature]')];
  const dirty = new Set();
  let originalBorder = null, inheritLigatures = false;
  const modal = io.registerModal(el('fontAdvancedDialog'), {initialFocus: () => el('fontAdvancedTab').getAttribute('aria-selected') === 'true' ? el('fontAdvancedTab') : el('fontTab'), fallbackFocus: io.fallbackFocus});
  function tab(advanced) {
    for (const [id,selected] of [['fontTab',!advanced],['fontAdvancedTab',advanced]]) { el(id).setAttribute('aria-selected',String(selected)); el(id).classList.toggle('is-active',selected); el(id).tabIndex = selected ? 0 : -1; }
    el('fontEffectsPanel').hidden = advanced; el('fontMetricsPanel').hidden = !advanced;
  }
  for (const [id,advanced] of [['fontTab',false],['fontAdvancedTab',true]]) {
    el(id).addEventListener('click',() => tab(advanced));
    el(id).addEventListener('keydown',event => { if (['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) { event.preventDefault(); const next = event.key === 'Home' ? false : event.key === 'End' ? true : !advanced; tab(next); el(next ? 'fontAdvancedTab':'fontTab').focus(); } });
  }
  for (const input of inputs) input.addEventListener('change',() => { dirty.add(input.dataset.character); input.indeterminate = false; });
  for (const input of ligatures) input.addEventListener('change',() => { inheritLigatures = false; dirty.add('ligatures'); input.indeterminate = false; });
  for (const id of ['font-border','font-borderColor','font-borderWidth']) el(id).addEventListener('change',() => { dirty.add('border'); dirty.add(id); el('font-border').setCustomValidity(''); reflectBorder(); });
  function reflectBorder() { const style = el('font-border').value; for (const id of ['font-borderColor','font-borderWidth']) el(id).disabled = !style || style === 'none' || style === '__mixed'; el('font-borderImportedNote').hidden = style !== 'imported'; }
  el('font-ligature-reset').addEventListener('click',() => { inheritLigatures = true; dirty.add('ligatures'); for (const input of ligatures) { input.checked = false; input.indeterminate = false; } });
  function open(advanced = false) {
    if (!io.getSelection()) return;
    dirty.clear();
    const state = JSON.parse(io.getDoc().selectionCharacterFormat(...io.getEndpoints()));
    const values = {...state.values, ...getPending()};
    for (const input of inputs) {
      const key = input.dataset.character, mixed = state.mixed.includes(key) && !(key in getPending());
      if (input.type === 'checkbox') { input.indeterminate = mixed; input.checked = values[key] === true; }
      else { input.value = mixed && input.tagName === 'SELECT' ? '__mixed' : mixed || values[key] == null ? '' : String(UNITS[key] ? values[key] / UNITS[key] : values[key]); input.placeholder = mixed ? t('indentSpecial.mixed') : t('fontAdvanced.inherit'); if (input.tagName === 'SELECT') input.querySelector('[value="__mixed"]').hidden = !mixed; }
    }
    inheritLigatures = values.ligatures == null;
    const bits = LIGATURE_MODES.indexOf(values.ligatures);
    for (const input of ligatures) { input.indeterminate = state.mixed.includes('ligatures'); input.checked = bits >= 0 && !!(bits & Number(input.dataset.ligature)); }
    const border = values.border; originalBorder = border;
    const styles = [...el('font-border').options].map(option => option.value);
    // An unsupported imported border remains untouched unless the user edits it.
    el('font-border').querySelector('[value="__mixed"]').hidden = !state.mixed.includes('border');
    el('font-border').value = state.mixed.includes('border') ? '__mixed' : border ? styles.includes(border.style) ? border.style : 'imported' : ''; el('font-border').setCustomValidity('');
    el('font-borderColor').value = border?.color ? `#${[border.color.r,border.color.g,border.color.b].map(v => v.toString(16).padStart(2,'0')).join('')}` : '#000000';
    el('font-borderWidth').value = String((border?.sizeEighthPoints ?? 8) / 8);
    reflectBorder(); tab(advanced); modal.open();
  }
  for (const id of ['fontAdvancedCancel','fontAdvancedClose']) el(id).addEventListener('click',() => modal.close());
  form.addEventListener('submit',async event => {
    event.preventDefault();
    for (const input of inputs) if (dirty.has(input.dataset.character) && !input.reportValidity()) return;
    const patch = {};
    for (const input of inputs) {
      const key = input.dataset.character;
      if (dirty.has(key)) patch[key] = input.type === 'checkbox' ? input.checked : UNITS[key] ? characterNumber(key,input.value) : input.value || null;
    }
    if (dirty.has('ligatures')) patch.ligatures = inheritLigatures ? null : LIGATURE_MODES[ligatures.reduce((bits,input) => bits | (input.checked ? Number(input.dataset.ligature) : 0),0)];
    if (dirty.has('border')) {
      for (const id of ['font-borderWidth']) if (dirty.has(id) && !el(id).reportValidity()) return;
      const style = el('font-border').value;
      if (!style) patch.border = null;
      else if (style === 'none') patch.border = {style:'none'};
      else {
        const args = borderArguments('all',el('font-borderColor').value,style,dirty.has('font-borderWidth') || !originalBorder ? el('font-borderWidth').value : '1',0);
        if (!args) { el('font-border').setCustomValidity(el('font-borderImportedNote').textContent); el('font-border').reportValidity(); return; }
        patch.border = {...originalBorder,style};
        if (!originalBorder || dirty.has('font-borderWidth')) patch.border.sizeEighthPoints = args[5];
        if (!originalBorder || dirty.has('font-borderColor')) { patch.border.color = {r:args[2],g:args[3],b:args[4]}; delete patch.border.themeColor; }
        if (!originalBorder) patch.border.spacePoints = 0;
      }
    }
    if (Object.keys(patch).length) await apply(patch);
    modal.close();
  });
  function commands() {
    const available = !!io.getSelection() && canEdit();
    const toggle = ['allCaps','smallCaps','doubleStrike','outline','shadow','emboss','imprint'].map(key => ({
      id:`format.character.${key}`,label:t(key === 'outline' ? 'ui.outline.label':`fontAdvanced.${key}`),group:'Format',kw:`font character ${key}`,enabled:available,disabledReason:t(canEdit() ? 'paragraph.caretRequired':'viewingBanner.viewingDocumentReadOnly'),
      run:() => { const state = JSON.parse(io.getDoc().selectionCharacterFormat(...io.getEndpoints())); return apply({[key]:!(getPending()[key] ?? state.values[key] ?? false)}); },
    }));
    return [...toggle,...[['dialog','fontAdvanced.command',false],['advanced','fontAdvanced.advanced',true],['border','fontAdvanced.border',false],['emphasis','fontAdvanced.emphasis',false],['spacing','fontAdvanced.spacing',true],['scale','fontAdvanced.scale',true],['position','fontAdvanced.position',true],['kerning','fontAdvanced.kerning',true],['ligatures','fontAdvanced.ligatures',true]].map(([id,key,advanced]) => ({id:`format.character.${id}`,label:t(key),group:'Format',kw:'font character formatting advanced',enabled:available,disabledReason:t(canEdit() ? 'paragraph.caretRequired':'viewingBanner.viewingDocumentReadOnly'),run:() => open(advanced)}))];
  }
  function reflect() {
    const button = el('fontAdvancedBtn');
    button.disabled = !io.getSelection() || !canEdit();
    button.title = button.disabled ? t(canEdit() ? 'paragraph.caretRequired':'viewingBanner.viewingDocumentReadOnly') : t('fontAdvanced.command');
  }
  io.onButton(el('fontAdvancedBtn'),() => open());
  reflect();
  return {open,commands,reflect};
}
