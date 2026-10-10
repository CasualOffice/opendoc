import test from 'node:test';
import assert from 'node:assert/strict';
import { borderArguments } from '../src/paragraph_advanced.mjs';
import { stopsFromFlat } from '../src/tab_stops_dialog.mjs';
test('border choice preserves edge, style, RGB, eighth points and padding', () => {
  assert.deepEqual(borderArguments('between', '#12ab34', 'double', '1.5', '7'), ['between','double',18,171,52,12,7]);
  assert.equal(borderArguments('box','#000000','single','',0), null);
  assert.equal(borderArguments('box','#000000','single',1,'1.5'), null);
  assert.equal(borderArguments('box','#000000','single',13,0), null);
  assert.equal(borderArguments('box','#000000','triple',1,0), null);
});
test('leader triples survive sorting and duplicate replacement without becoming alignment', () => {
  assert.deepEqual(stopsFromFlat([2880,2,3,1440,0,1,2880,3,2],3), [
    {position:1440,align:'start',leader:1}, {position:2880,align:'decimal',leader:2},
  ]);
});
import { createParagraphAdvanced } from '../src/paragraph_advanced.mjs';
test('mixed inspector edits only the chosen pagination property through the selection route', async () => {
  const ids = ['paraBorderImportedNote','paraBorderStyle','paraBorderWidth','paraBorderPadding','runDirectionReason','runDirectionRtl','runDirectionLtr','paraDirection','paraWidow','paraContextual','paraOutline','paraClearAll','paraBorderBetween','paragraphPropertiesPanel'];
  const controls = Object.fromEntries(ids.map((id) => [id, {value:'', checked:false, listeners:{}, addEventListener(event,fn){ this.listeners[event]=fn; }, setAttribute(){}, querySelectorAll(){ return []; }}]));
  const previous = globalThis.document;
  globalThis.document = {getElementById:(id)=>controls[id], activeElement:null};
  const calls = [];
  let borders = {between:{style:'none'}};
  const doc = {
    selectionParagraphAdvanced(...range) { assert.deepEqual(range,['a',0,'b',2]); return JSON.stringify({rtlState:2,widowState:2,contextualState:1,outlineLevel:2,outlineMixed:true}); },
    paragraphBorders(){ return JSON.stringify(borders); },
    textDirectionState(){ return 0; },
    setParagraphWidowControl(...args){ calls.push(args); },
  };
  try {
    const ui = createParagraphAdvanced({getDoc:()=>doc,getSelection:()=>({focus:{node:'b'}}),getEndpoints:()=>['a',0,'b',2],onButton:(el,fn)=>el.addEventListener('click',fn),runToolbarEdit:async(fn,options)=>{assert.deepEqual(options,{paragraphLevel:true});fn('a',0,'b',2);}});
    ui.reflect();
    assert.equal(controls.paraWidow.indeterminate,true);
    assert.equal(controls.paraDirection.value,'');
    assert.equal(controls.paraOutline.value,'');
    controls.paraWidow.checked=true;
    controls.paraWidow.listeners.change();
    await Promise.resolve();
    assert.deepEqual(calls,[['a',0,'b',2,true]]);
    borders = {between:{style:'double',sizeEighthPoints:12,spacePoints:7}};
    ui.reflect();
    assert.equal(controls.paraBorderStyle.value,'double');
    assert.equal(controls.paraBorderWidth.value,'1.5');
    assert.equal(controls.paraBorderPadding.value,'7');
    borders = {top:{style:'triple',sizeEighthPoints:12}};
    ui.reflect();
    assert.equal(controls.paraBorderStyle.value,'imported');
    assert.equal(controls.paraBorderImportedNote.hidden,false);
    borders = {top:{style:'dashSmallGap',sizeEighthPoints:12}};
    ui.reflect();
    assert.equal(controls.paraBorderStyle.value,'dashed');
    assert.equal(controls.paraBorderImportedNote.hidden,true);
  } finally { globalThis.document=previous; }
});
