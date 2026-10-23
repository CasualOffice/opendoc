// Optional local development host. Production hosts supply authentication and durable storage.
//
// OPENDOC_SHARE_ACCESS=granted runs every room as a GRANTED room (`docs/175` §4.1): the relay
// verifies an EdDSA JWT against this host's public key, refuses a join whose identity is not
// the grant's subject, and shows the grant's name. The key is a DEVELOPMENT key generated at
// start-up, and "who you are" is a `opendoc_dev_user` cookie (`<id>|<display name>`) that
// anybody can set: this host has no authentication, and a real host signs grants only for the
// person its own session authenticated.
import http from "node:http";
import net from "node:net";
import { spawn, spawnSync } from "node:child_process";
import { generateKeyPairSync, randomUUID, sign } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve, extname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const relay = process.env.OPENDOC_RELAY_BINARY || resolve(root, "target/debug/opendoc-relay");
const directory = await mkdtemp(resolve(tmpdir(), "opendoc-share-"));
const rooms = new Map();
const children = new Set();
const MAX_BYTES = 64 * 1024 * 1024;
const GRANTED = process.env.OPENDOC_SHARE_ACCESS === "granted";
// DEVELOPMENT KEY. Generated per process and never written anywhere; the relay is handed only
// the public half, so it can verify grants and never mint one.
const devKey = GRANTED ? generateKeyPairSync("ed25519") : null;
const devPublicKey = devKey ? devKey.publicKey.export({ format: "jwk" }).x : "";
const b64url = (value) => Buffer.from(value).toString("base64url");
/** A compact JWS (EdDSA) carrying the grant claims `JwtVerifier` reads. */
function grantFor(roomId, cookie) {
  const raw = /(?:^|;\s*)opendoc_dev_user=([^;]+)/.exec(cookie || "")?.[1];
  if (!raw) return null;
  const [sub, name] = decodeURIComponent(raw).split("|");
  if (!sub) return null;
  const claims = { sub, aud: roomId, exp: Math.floor(Date.now() / 1000) + 600, role: "editor" };
  if (name) claims.name = name;
  const signed = `${b64url(JSON.stringify({ alg: "EdDSA", typ: "JWT" }))}.${b64url(JSON.stringify(claims))}`;
  return `${signed}.${b64url(sign(null, Buffer.from(signed), devKey.privateKey))}`;
}
const hostScript = `globalThis.OPENDOC_COLLABORATION = {
 ${GRANTED ? `async credentialsFor({room}) {
  const id = new URL(room).pathname.split('/').pop();
  const result = await fetch('/api/shares/'+id+'/grant', {method:'POST'});
  if (!result.ok) throw new Error('The share host would not issue a grant.');
  return { grant: await result.text() };
 },` : ""}
 async createShare({bytes}) {
  const result = await fetch('/api/shares', {method:'POST', body:bytes});
  if (!result.ok) throw new Error('The share host refused the document.');
  return result.json();
 },
 async snapshotFor({room, minimumRevision}) {
  const id = new URL(room).pathname.split('/').pop();
  const result = await fetch('/api/shares/'+id+'/snapshot');
  if (!result.ok) throw new Error('The share host has no snapshot.');
  const revision = Number(result.headers.get('X-Document-Revision'));
  if (revision < minimumRevision) throw new Error('The checkpoint has not reached the required revision yet. Retry recovery.');
  return {revision,bytes:new Uint8Array(await result.arrayBuffer())};
 },
 async publishSnapshot({room,revision,bytes}) {
  const id = new URL(room).pathname.split('/').pop();
  const result = await fetch('/api/shares/'+id+'/snapshot?revision='+revision, {method:'POST',body:bytes});
  if (!result.ok) throw new Error('Checkpoint was refused.');
 }
};`;
async function bytes(request) {
  let count = 0; const chunks = [];
  for await (const chunk of request) {
    count += chunk.length;
    if (count > MAX_BYTES) throw new Error("Document exceeds host limit");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}
function normalized(buffer) {
  const value = JSON.parse(buffer.toString('utf8'));
  // The engine performs full schema validation on open; this host bounds storage.
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid document');
}
let creating = 0;
async function createRoom(seed) {
  if (rooms.size + creating >= 16) throw new Error('Local host room limit reached');
  creating++;
  try { return await createRoomInner(seed); } finally { creating--; }
}
async function createRoomInner(seed) {
  if (rooms.size >= 16) throw new Error('Local host room limit reached');
  normalized(seed);
  const id = randomUUID();
  const journal = resolve(directory, `${id}.journal`);
  const created = spawnSync(relay, ['create', journal], {encoding:'utf8'});
  if (created.status !== 0) throw new Error('Build opendoc-relay before starting the Share host.');
  const listener = net.createServer();
  await new Promise((done, fail) => { listener.once('error',fail); listener.listen(0,'127.0.0.1',done); });
  const port = listener.address().port;
  await new Promise(done => listener.close(done));
  const access = GRANTED ? ['granted', devPublicKey, id] : ['editor'];
  const child = spawn(relay, ['serve',journal,`127.0.0.1:${port}`,...access], {stdio:['ignore','pipe','pipe']});
  children.add(child); child.once('exit',() => children.delete(child));
  await new Promise((done, fail) => {
    const deadline = setTimeout(() => { child.kill(); fail(new Error('Relay startup timed out')); },10000);
    child.once('error', error => { clearTimeout(deadline); fail(error); });
    child.once('exit', () => { clearTimeout(deadline); fail(new Error('Relay did not start')); });
    child.stderr.on('data', data => { if (String(data).includes('relaying')) { clearTimeout(deadline); done(); } });
    child.stdout.on('data', data => { if (String(data).includes('relaying')) { clearTimeout(deadline); done(); } });
  });
  const room = {id,port,revision:0,seed,snapshot:seed};
  rooms.set(id,room);
  await writeFile(resolve(directory, `${id}.json`),seed);
  return room;
}
const server = http.createServer(async (request, response) => {
  try {
    const origin = `http://127.0.0.1:${server.address().port}`;
    const url = new URL(request.url,origin);
    // Local host is same-origin only; it deliberately has no cross-origin API.
    if (request.headers.origin && request.headers.origin !== origin) { response.writeHead(403); response.end(); return; }
    response.setHeader('Cache-Control','no-store');
    if (url.pathname === '/api/shares' && request.method === 'POST') {
      const room = await createRoom(await bytes(request));
      response.setHeader('Content-Type','application/json');
      response.end(JSON.stringify({room:`ws://127.0.0.1:${server.address().port}/rooms/${room.id}`,documentUrl:`${origin}/documents/${room.id}.json`,revision:0})); return;
    }
    const granting = /^\/api\/shares\/([^/]+)\/grant$/.exec(url.pathname);
    if (granting && GRANTED && request.method === 'POST') {
      // Issued for whoever the session cookie says; never logged, never stored.
      const token = rooms.has(granting[1]) ? grantFor(granting[1], request.headers.cookie) : null;
      if (!token) { response.writeHead(403); response.end(); return; }
      response.setHeader('Content-Type','text/plain'); response.end(token); return;
    }
    const checkpoint = /^\/api\/shares\/([^/]+)\/snapshot$/.exec(url.pathname);
    if (checkpoint) {
      const room = rooms.get(checkpoint[1]);
      if (!room) { response.writeHead(404); response.end(); return; }
      if (request.method === 'POST') {
        const revision = Number(url.searchParams.get('revision'));
        const snapshot = await bytes(request); normalized(snapshot);
        if (!Number.isSafeInteger(revision) || revision < 0) throw new Error('Invalid revision');
        if (revision > room.revision) { room.snapshot = snapshot; room.revision = revision; }
        response.writeHead(204); response.end(); return;
      }
      if (request.method !== 'GET') { response.writeHead(405); response.end(); return; }
      response.setHeader('X-Document-Revision',String(room.revision));
      response.setHeader('Content-Type','application/json'); response.end(room.snapshot); return;
    }
    const document = /^\/documents\/([^/]+)\.json$/.exec(url.pathname);
    if (document) {
      const room = rooms.get(document[1]);
      if (!room) { response.writeHead(404); response.end(); return; }
      response.setHeader('Content-Type','application/json'); response.end(room.seed); return;
    }
    if (request.method !== 'GET') { response.writeHead(405); response.end(); return; }
    const path = resolve(root,'webapp','.' + decodeURIComponent(url.pathname === '/' ? '/editor.html' : url.pathname));
    if (!path.startsWith(resolve(root,'webapp') + '/')) { response.writeHead(403); response.end(); return; }
    let body = await readFile(path);
    const mime = {'.html':'text/html','.mjs':'text/javascript','.js':'text/javascript','.css':'text/css','.wasm':'application/wasm','.json':'application/json'}[extname(path)] || 'application/octet-stream';
    if (path.endsWith('/editor.html')) body = Buffer.from(body.toString().replace('<head>',`<head><script>${hostScript}</script>`));
    response.setHeader('Content-Type',mime); response.end(body);
  } catch (error) { response.writeHead(error.code === 'ENOENT' ? 404 : 400, {'Content-Type':'text/plain'}); response.end(error.message); }
});
server.on('upgrade', (request, socket, head) => {
  const room = rooms.get(/^\/rooms\/([^/?]+)$/.exec(request.url)?.[1]);
  const expectedOrigin = `http://127.0.0.1:${server.address().port}`;
  if (!room || request.headers.origin !== expectedOrigin) { socket.destroy(); return; }
  const upstream = net.connect(room.port,'127.0.0.1', () => {
    upstream.write(`${request.method} ${request.url} HTTP/${request.httpVersion}\r\n` + request.rawHeaders.reduce((out,value,index,array) => index % 2 ? out : out + `${value}: ${array[index + 1]}\r\n`, '') + '\r\n');
    if (head.length) upstream.write(head);
    socket.pipe(upstream).pipe(socket);
  });
  upstream.on('error',() => socket.destroy()); socket.on('error',() => upstream.destroy()); socket.on('close',() => upstream.destroy());
});
server.listen(Number(process.env.OPENDOC_SHARE_PORT || 8100),'127.0.0.1', () => console.log(`Share host listening on ${server.address().port}`));
async function stop() { for (const child of children) child.kill(); server.close(); await rm(directory,{recursive:true,force:true}); process.exit(0); }
process.once('SIGTERM',stop); process.once('SIGINT',stop);
