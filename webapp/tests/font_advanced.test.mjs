import test from 'node:test';
import assert from 'node:assert/strict';
import { characterNumber, LIGATURE_MODES, typeArmedCharacter } from '../src/font_advanced.mjs';
test('font dialog preserves exact model units and refuses invalid authoring',() => {
  assert.equal(characterNumber('characterSpacingTwips','1.15'),23);
  assert.equal(characterNumber('positionHalfPoints','-1.5'),-3);
  assert.equal(characterNumber('kerningHalfPoints','12'),24);
  assert.equal(characterNumber('characterScalePercent',''),null);
  for (const [key,value] of [['characterScalePercent','0'],['characterSpacingTwips','Infinity'],['positionHalfPoints','0.25'],['kerningHalfPoints','-1']]) assert.throws(() => characterNumber(key,value),RangeError);
  assert.equal(new Set(LIGATURE_MODES).size,16);
});
test('armed typing forwards the character patch in the same editing or review gesture',() => {
  for (const mode of ['editing','suggesting']) {
    let args; typeArmedCharacter({typeCharacterText(...v) {args=v;}},{node:'p',offset:3},'x',{character:{outline:true},bold:true},8,mode);
    assert.equal(args[3],true); assert.equal(args[12],8); assert.deepEqual(JSON.parse(args[15]),{outline:true}); assert.equal(args[16],mode==='suggesting');
  }
});
test('arming a character field preserves pending basic formatting and viewing refuses it',async() => {
  const {applyCharacterPatch}=await import('../src/font_advanced.mjs');
  let pending={bold:true,character:{shadow:true}}, calls=0, viewing=false;
  const io={blockMutationInViewing:()=>viewing,hasRange:()=>false,getPendingFormat:()=>pending,setPendingFormat:value=>{pending=value;},updateToolbar:()=>{calls++;}};
  applyCharacterPatch(io,{allCaps:true});assert.deepEqual(pending,{bold:true,character:{shadow:true,allCaps:true}});assert.equal(calls,1);
  viewing=true;applyCharacterPatch(io,{outline:true});assert.equal(pending.character.outline,undefined);assert.equal(calls,1);
});
