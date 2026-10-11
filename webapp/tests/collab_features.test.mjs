import test from 'node:test';
import assert from 'node:assert/strict';
import { sharedDocument, shareLink, createShare } from '../src/share_flow.mjs';
import { createRecovery } from '../src/collab_recovery.mjs';
import { createPresence } from '../src/collab_presence.mjs';
const t = (key) => key;
const base = 'https://docs.example/editor.html?grant=secret&author=private';
test('share links bind the seed revision and exclude grants and private query parameters', () => {
  const link = new URL(shareLink(base,{room:'wss://docs.example/rooms/one',documentUrl:'/documents/one.json',revision:123}));
  assert.equal(link.searchParams.get('revision'),'123');
  assert.deepEqual([...link.searchParams.keys()],['room','document','revision']);
  assert.equal(sharedDocument(base,'https://other.example/doc.json'),null);
  assert.equal(sharedDocument(base,'https://user:password@docs.example/doc.json'),null);
  assert.equal(shareLink(base,{room:'ws://docs.example/one',documentUrl:'/one.json',revision:0}),null);
  assert.equal(shareLink(base,{room:'wss://docs.example/one',documentUrl:'/one.json',revision:NaN}),null);
});
test('Share persists the exact document before joining and supports manual copy', async () => {
  const events = [];
  const doc = {revision:7,collabSnapshot:() => new Uint8Array([1,2])};
  const share = createShare({document:() => doc,provider:() => ({createShare:async ({bytes}) => {
    assert.deepEqual([...bytes],[1,2]); events.push('persist');
    return {room:'wss://docs.example/one',documentUrl:'/one.json',revision:0};}}),base:() => base,name:() => 'a',joined:async () => events.push('join'),
    clipboard:() => undefined,showLink:() => events.push('dialog'),status:(key) => events.push(key),t});
  assert.ok(await share.share());
  assert.deepEqual(events,['persist','join','dialog','collab.shareCopyManually']);
});
test('Share refuses a stale seed if editing continued during host persistence', async () => {
  const doc = {revision:1,collabSnapshot:() => new Uint8Array()};
  let joined = false;
  const share = createShare({document:() => doc,provider:() => ({createShare:async () => {doc.revision++; return {};}}),base:() => base,name:() => '',joined:() => {joined=true;},status(){},t});
  await share.share(); assert.equal(joined,false);
});
test('recovery leaves work intact when the host has no snapshot, and retries a checked snapshot', async () => {
  let host = {}, restored = 0, resynced = 0;
  const doc = {collabRecover(bytes,revision) { assert.equal(revision,9); restored++; return JSON.stringify({replayed:2,retained:1}); }};
  const recovery = createRecovery({document:() => doc,provider:() => host,room:() => 'room',transport:() => ({resync:() => resynced++}),recovered:async () => {},status(){},t});
  recovery.receive({kind:'recoveryRequired',revision:8});
  assert.equal(recovery.needed(),true); assert.equal(restored,0);
  host = {snapshotFor:async () => ({revision:7,bytes:new Uint8Array([1])})};
  await recovery.receive({kind:'recoveryRequired',revision:8}); assert.equal(restored,0);
  host = {snapshotFor:async () => ({revision:9,bytes:new Uint8Array([1])})};
  await recovery.receive({kind:'recoveryRequired',revision:8});
  assert.equal(restored,1); assert.equal(resynced,1); assert.equal(recovery.needed(),false);
});
test('an obsolete recovery cannot replace a newly opened document', async () => {
  let resolve, restored = false, doc = {collabRecover(){restored=true;}};
  const recovery = createRecovery({document:() => doc,provider:() => ({snapshotFor:() => new Promise(done => {resolve=done;})}),room:() => 'room',transport(){},recovered(){},status(){},t});
  const pending = recovery.receive({kind:'recoveryRequired',revision:0});
  doc = {}; resolve({revision:1,bytes:new Uint8Array([1])}); await pending; assert.equal(restored,false);
});
test('presence is throttled and disconnected cursors are cleared without queuing', () => {
  let callback, sends = 0, clears = 0;
  const roster = {free(){},clear(){clears++;},caretFrame:() => new Uint8Array([1]),receiveFrame(){}};
  const presence = createPresence({newRoster:() => roster,document:() => ({}),transport:() => ({state:() => ({name:'connected'}),awareness:() => {sends++;return true;}}),selection:() => ({anchor:{node:'1',offset:0},focus:{node:'1',offset:1}}),paint(){},t,setTimer:fn => {callback=fn;return 1;},clearTimer(){}});
  presence.open(true); presence.changed(); presence.changed(); assert.equal(sends,0); callback(); assert.equal(sends,1);
  presence.state({name:'reconnecting'}); assert.equal(clears,1);
});

test('presence consumes each arrival map before deferred painting can skip a revision', () => {
  const calls = []; const doc = {revision:1};
  const roster = {free(){},receiveFrame:() => calls.push('frame'),remoteSelections:current => {calls.push(current.revision);return '[]';}};
  const presence = createPresence({newRoster:() => roster,document:() => doc,transport(){},selection(){},paint(){},t});
  presence.open(true);presence.frame(new Uint8Array([1]));doc.revision=2;presence.frame(new Uint8Array([2]));
  assert.deepEqual(calls,['frame',1,'frame',2]);
});
