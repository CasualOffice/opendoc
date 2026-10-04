# 162 — Deployment: container images, compose, and configuration

**Status:** Shipped with the images it documents, 2026-10-04. Closes `109` RM-11.
**Opened:** 2026-10-04.
**Scope:** `Dockerfile.editor`, `Dockerfile.relay`, `docker-compose.yml`,
`.dockerignore`, `deploy/editor-nginx.conf`, and the deployment half of the embedding
guide. It changes **no engine, editor or relay code**: everything here reads the product
as it is. `server/src/**` is owned by the co-editing lane and was read, not edited.

**Honesty basis:** every capability, number and limit below is cited to the file that
decides it, and `webapp/tests/deployment_contract.test.mjs` re-derives the ones that can
drift — the relay's subcommands and argument order, the five role names, the ports, the
pinned toolchain versions and the participant ceiling — from the source rather than from
this document. `SKILL.md` §9 exists because published pages here have carried fabricated
claims; a deployment guide is a published claim, so §8 of this document enumerates what
these images do **not** do rather than leaving it to be discovered in production.

---

## 1. There are two deployments, and they are not a stack

The editor is a static, local-first web app that needs **no server to function**, and the
collaboration relay is **optional** and holds **no document**. That is the only thing in
this document worth reading twice, and everything below follows from it.

| | **The editor** | **The relay** |
| --- | --- | --- |
| What it is | A static, local-first web app | An optional ordering service |
| Needs the other? | **No** | Needs editors, obviously |
| Holds a document? | In the browser tab | **No** — holds an ordered log of chunks |
| Container shape | A static file server | One `std` TCP listener |
| Image | `Dockerfile.editor` | `Dockerfile.relay` |
| Compose | `editor` (default) | `relay` (behind a profile) |
| Required for the product | **Yes** | **No** |

### 1a. One container by default, and why there are not four

**`docker compose up` starts one container.** The compose file declares **two services**
and only one of them is outside a profile; §2 states both numbers and
`webapp/tests/deployment_contract.test.mjs` re-derives them from the file.

It declared **four** when this document first shipped, and that was wrong in a way worth
recording rather than quietly fixing. Two of the four — `relay-create` and `relay-inspect`
— were not services at all: `create` and `inspect` are one-shot administrative commands
that run, print and exit, and a compose service that exits immediately is a row in
`docker compose ps` that is never up, plus an invitation to `up` it. They are
`docker compose run --rm relay <subcommand>` invocations now (§5.2, §6.3) against the one
relay service: the same image, the same journal volume, no published port, one entry in the
file. Nothing was lost by moving them — the guard that used to read their compose
`command:` lines now reads the invocations **here**, against the same `server/src/main.rs`,
and fails if this document stops publishing one of them (§15).

So the count a deployer reasons about is **one container, or two if they want a room
several people edit at once.**

> **The competitor comparison below is recollection, not evidence, and is fenced so that
> nothing downstream can cite it as sourced.** `/Users/sachin/Desktop/melp/reference/`
> holds ONLYOFFICE's `sdkjs` and `web-apps` — both **client** repositories — and a search
> of them for `Dockerfile`, `docker-compose` or any image definition returns only
> syntax-highlighting fixtures in `web-apps/vendor/monaco` and `web-apps/vendor/ace`. No
> competitor's packaging is checked out on this machine, so **nothing in this paragraph was
> read from a source.** `SKILL.md` §9 exists because this repository has published
> fabricated competitive claims twice; treat the next two sentences as a hypothesis to
> verify before they inform a decision.
>
> ONLYOFFICE Document Server is distributed as a single container image, and Collabora
> Online / CODE likewise — in both cases, as recalled, a single image that runs a
> supervisor over several processes, because **their server is the product** and a browser
> cannot open a document without it.
>
> If that recollection holds, the comparison does not favour a merge here, it favours the
> opposite reading: their one container is one that **must** run a server, and ours is one
> that runs **none**. Matching their service count by baking a relay into the editor's
> image would be matching the number while giving up the thing the number is evidence for.
> §3.5 is the decision, with the measured cost.

**The editor needs no server to function.** `webapp/build.sh` compiles
`crates/casual-doc-wasm` to WebAssembly and stages a flat static site beside it, so
opening a DOCX, laying it out, editing it and writing it back all happen in the tab. The
container for it is a file server; it could be a CDN, a bucket, or GitHub Pages — which is
where this project already publishes it.

**That is the structural advantage, not a packaging detail.** ONLYOFFICE's web client
cannot open a file offline: its format I/O is the native `x2t` converter and
`core/X2tConverter/build/` has only `Android/` and `Qt/` targets, with no WebAssembly
build, and they do not ship the native core as an embeddable library. A guide that
presented a relay as part of "installing opendoc" would be describing their architecture
and giving away the only position this project occupies.

**The relay is additive.** `server/` is a workspace member deliberately **not** under
`crates/`, and the test `nothing_under_crates_depends_on_the_relay` fails the build if
anything in `crates/` ever acquires a dependency on it. `server/src/lib.rs` gives the
reason: a relay that `crates/` could depend on would be a relay that could become required
by accident, one `use` at a time, "and the licence-and-embeddability position this project
occupies does not survive that."

So the two sections that follow are **alternatives, not steps**.

---

## 2. Derived constants

Every value in this table is asserted against its source by
`webapp/tests/deployment_contract.test.mjs`. None of them is maintained by hand here.

| Value | What it is | Source of truth |
| --- | --- | --- |
| `1.96.0` | The Rust toolchain both images pin | `rust-toolchain.toml` channel |
| `0.15.0` | The wasm-pack version the editor image installs | `.github/workflows/ci.yml` |
| `22` | The Node major the editor image builds with | `.github/workflows/ci.yml` |
| `8099` | Host port compose publishes for the editor | `docker-compose.yml` |
| `8080` | Port nginx listens on inside the editor container | `deploy/editor-nginx.conf` |
| `7070` | Host and container port for the relay | `docker-compose.yml` |
| `128` | Participants one room will hold | `MAX_PARTICIPANTS`, `crates/casual-doc-transaction/src/presence.rs` |
| `ODC-7010` | The refusal a full room answers with | `Refusal::RoomFull`, proven in `server/src/relay_tests.rs` |
| 5 | Roles `serve` accepts | `open_room_role`, `server/src/main.rs` |
| 2 | Services the compose file declares | `docker-compose.yml` |
| 1 | Containers a default `docker compose up` starts | `docker-compose.yml`, the services outside a profile |

---

## 3. Deploying the editor

### 3.1 Build and run it

```sh
docker build -f Dockerfile.editor -t opendoc/editor:dev .
docker run --rm -p 8099:8080 opendoc/editor:dev
# then open http://localhost:8099/editor.html
```

or, with compose, which is the same thing plus the hardening:

```sh
docker compose up --build editor
```

`docker compose up` with no service starts **only** the editor — one container, and the
only one outside a profile (§1a). The relay sits behind a profile precisely so that it
cannot be started by accident.

**Both images were built and run from the commit that reshaped this file**, because a
Dockerfile nobody has built is not a deliverable and the first version of these two shipped
unverified. What was observed, on arm64: `docker build` of both; `docker compose up` with no
profile starting **one** container, reported `(healthy)`; `/editor.html` answered `200`
`text/html`, 298,587 bytes, `/healthz` `200`, and `/pkg/casual_doc_wasm_bg.wasm` `200`
**`application/wasm`**, 24,995,252 bytes — the content type §3.3's location block exists to
pin, proven in the built image rather than in the config file. The relay half:
`docker compose run --rm relay create …` printing `created a room at …` and **failing on a
second run** with `journal i/o: File exists (os error 17)`; the room up and `(healthy)`,
logging `replayed 0 ordered chunks` then `relaying … on 0.0.0.0:7070`, and accepting a TCP
connection on the published port; `docker compose run --rm relay inspect …` on the stopped
room printing exactly the two lines §6.3 publishes; `docker run opendoc/relay:dev` with no
arguments printing the usage line, and `serve` without a role exiting **2**. The editor's
build also ran `webapp/build.sh` end to end inside the image, so every generator's
`--check` passed against this commit's committed artefacts.

### 3.2 What is in the image, and what is not

The build is three stages — `node`, `build`, `runtime` — and the last one is
`nginx:1.29-alpine`. There is **no Rust, no cargo, no Node, no Python and no source** in the
runtime image: the toolchain exists in the earlier stages and nothing is copied forward but
the staged site. (`webapp/tests/deployment_contract.test.mjs` counts the stages, because
the first draft of this paragraph said four.)

The builder runs `webapp/build.sh` unmodified. That matters for a reason beyond
convenience: the script ends with five generators run in `--check` mode — the embedding
guide, the white-label artefacts, the static pages, the published reference pages and the
crawler manifests — so **the image cannot be built from a checkout whose generated files
have drifted from their sources.** The same gate CI runs is enforced as a side effect of
building the thing.

### 3.3 Fonts are self-hosted, and the server must not interfere

`webapp/src/fonts.css` self-hosts Inter and Material Symbols Outlined from
`../assets/fonts/*.woff2`, and `webapp/tests/chrome_fonts.test.mjs` asserts the stylesheet
holds no reference to either Google Fonts host — the two hostnames are in that test rather
than in this sentence, because `webapp/tests/doc_pages.test.mjs` refuses a published page
that names one and it was right to refuse this one. That guard is what keeps the editor's
chrome working with no network, so the image carries the woff2 files and
`deploy/editor-nginx.conf` does exactly one thing with them: hands back the bytes the build
produced. **No `sub_filter`, no CDN rewrite, no asset host.** A rewrite there would undo
the guarantee in the one place no test is looking.

`application/wasm` is declared explicitly in the same file. nginx has shipped it in
`mime.types` since 1.25, but a silent fallback to `application/octet-stream` makes
`WebAssembly.instantiateStreaming` refuse the module, and the failure looks like a blank
editor with one console line.

### 3.4 Offline, precisely

**The chrome and the application are fully offline.** The HTML, CSS, the WebAssembly
module, the nineteen locale catalogues, the spelling word lists, the glossary and the two
chrome typefaces are all served from the image.

**Document text fonts are not, in the default build, and this is the one place where
"local-first" needs a qualifier.** `webapp/build.sh` builds the module with
`--features web-host-fonts`; `crates/casual-doc-wasm/Cargo.toml` has that feature on by
default, and `crates/casual-doc-layout/src/fonts.rs` records what it does — the
`external-web-fonts` WASM build "omits these four byte blobs". `webapp/src/web_fonts.mjs`
then fetches Roboto and the Noto families over the network from four commit-pinned
revisions on a public CDN — that file holds the four hostnames and revisions, and this one
does not repeat them — and the CJK and emoji faces are fetched on demand when
`missingCoverage()` asks for them.

So a container on an air-gapped network serves a working editor whose **document** text
falls back to the faces embedded in the module. Two honest options:

1. **Mirror the fonts.** The URLs are commit-pinned in `webapp/src/web_fonts.mjs`; a
   deployment that wants them local serves them from its own origin and changes those
   constants.
2. **Embed them.** Build the module without the feature — the four Roboto blobs go back
   into the bundle, at roughly 2 MB that every visitor downloads once, which is exactly the
   cost the feature was added to avoid.

Neither is wired into these images, because choosing for an operator would be choosing
between a 2 MB download and a CDN dependency on their behalf. `109` HF-176 (no CJK or
Arabic or Indic face is bundled) and HF-132 (the font-provisioning decision) are the rows
this sits behind.

### 3.5 Why the relay is not in this image — the decision, with its cost measured

The question was asked directly: should the editor and the relay be **one image**, since a
single `docker run` is what a deployer expects? The answer is **no, and here is the
arithmetic rather than a preference.** Every figure below is a reading taken on one arm64
machine — `docker image inspect --format '{{.Size}}'` for the images, which were **built
from the files in this commit**, and `ls -l` for the binary, copied out of the image it
ships in.

| Measured | Bytes | |
| --- | --- | --- |
| `opendoc/editor:dev` | 141,739,417 | the single-user default today |
| `opendoc/relay:dev` | 144,059,613 | the optional second deployment |
| `opendoc-relay`, the binary | 5,706,472 | stripped by `[profile.release]`, inside that image |
| `nginx:1.29-alpine` | 91,758,413 | the editor's runtime base |
| `nginx:1.29` (bookworm) | 255,159,672 | the nearest Debian base a glibc binary can run on |

**The binary cannot simply be copied into the editor image.** `file` reports it as an
`ELF 64-bit LSB pie executable, ARM aarch64, dynamically linked, interpreter
/lib/ld-linux-aarch64.so.1`, needing `GLIBC_2.28`/`2.33`; the editor's runtime is Alpine,
which is musl and has no such loader. So a combined image is one of two concrete changes,
and both cost something:

1. **Move the editor's runtime to Debian** so the existing binary runs beside nginx:
   141,739,417 − 91,758,413 + 255,159,672 + 5,706,472 ≈ **310,847,148 bytes**, against
   141,739,417 today. **+119% on the image every single-user deployment pulls**, to carry a
   server that deployment never starts.
2. **Build the relay for musl** so it fits the Alpine runtime. The marginal content cost
   is then about the binary itself — **≈ +4%** on 141,739,417, taking the measured glibc
   size as the estimate; the musl figure is **not measured here**, and a musl build also
   adds a target and a linker to the builder stage.

Option 2 is cheap enough that **size is not the reason**. The reasons are these:

- **The "no server" case must be no process, not a stopped one.** A single container that
  runs both is two processes under a supervisor, and then the editor's container has a
  server in it that is merely idle. A dormant-unless-a-flag entrypoint does satisfy "no
  process" — but as soon as the flag is on, nginx and the relay share one PID 1, one
  healthcheck, one restart policy and one user; if the relay dies, the container stays up
  because nginx is alive, and the collaboration failure is invisible to every orchestrator.
  That is a worse deployment than two containers, in the only case where a merge would
  have helped.
- **The editor image's root filesystem is read-only with a tmpfs on `/tmp`** (§4), which is
  possible *because* nothing in it writes. The relay writes a journal it must own, as uid
  10001, and the editor serves as uid 101. One image means one of those two guarantees
  goes.
- **`nothing_under_crates_depends_on_the_relay` is enforced in the build** for the reason
  `server/src/lib.rs` gives: a relay that could become required by accident, one `use` at
  a time. Shipping the relay inside the product's own image is that same accident at the
  packaging layer — the binary would be present on every deployment, and the next thing
  that reaches for it would find it there.

**What the decision is not.** It is not "a deployer composes two things". The default is
`docker compose up`: one container, no server, no volume, no journal. The second container
exists only for the second deployment, and `docker run -p 7070:7070 opendoc/relay:dev serve
…` is a single command when that is what somebody wants.

Recorded here rather than as an ADR on purpose: three other lanes are editing this tree, and
`SKILL.md` §5a.3 records two ADR-034s colliding and taking two published ADRs with them.
This is a packaging shape, and this document owns packaging; §14's table carries the row.

### 3.6 Serving it from something other than a container

The runtime stage is a directory of static files. `docker create` plus `docker cp` of
`/usr/share/opendoc/webapp` gives the same bytes for a bucket, a CDN or an existing web
server. The only requirements are the ones in §3.3: serve `.wasm` as `application/wasm`,
and do not rewrite font URLs.

---

## 4. Single user, and what it costs

One person editing their own documents needs the editor container and nothing else. No
relay, no volume, no database, no account.

The editor container stores nothing. `docker-compose.yml` mounts its root filesystem
**read-only** with a tmpfs on `/tmp`, which is possible only because
`deploy/editor-nginx.conf` puts every path nginx writes into `/tmp`. That is the strongest
statement available in a compose file that this deployment holds no document: there is
nowhere for one to go. Draft recovery and the personal dictionary live in the browser's own
IndexedDB, on the reader's machine.

---

## 5. Adding the relay

### 5.1 The interface, which is positional

`server/src/main.rs` matches on `std::env::args()` and reads **no environment variable at
all**:

```text
opendoc-relay create  <journal>
opendoc-relay inspect <journal>
opendoc-relay serve   <journal> <addr> <viewer|commenter|suggester|editor|owner>
```

That is the whole configuration surface. §9 records env-var support as a follow-up with
the exact change it needs; until then, a compose file that set `RELAY_ADDR` would be
documenting an interface that does not exist.

### 5.2 The host creates the room; the first client does not

`create` is a separate subcommand on purpose. `docs/152` records that the **host** creates
the room, not the first client, and `server/src/lib.rs` enforces it through what the API
makes possible: `Room::create` takes a host's decision and a path, and a client cannot call
it. `Journal::create` refuses to write over an existing journal, "because creating over a
room's order would destroy the only copy of it" — so `create` run twice fails, by design.

**It is a command, not a service.** `docker compose run` against the `relay` service
overrides its `command`, mounts the same journal volume and publishes no port, so it does
not collide with a running room; compose enables that service's own profile for `run`, so
no `--profile` flag is needed either.

```sh
# Once, before the first serve. Creates the durable log on the named volume.
docker compose run --rm relay create /var/lib/opendoc-relay/room.journal

# Then the room, with the editor:
docker compose --profile relay up --build
```

### 5.3 The role is required, and it is the room's ceiling

`serve` will not start without a role, and the binary says why: "a default would be a
permission nobody chose." The image therefore ships **no `CMD`** — a default in the image
would be exactly the default the binary refuses to have — so `docker run opendoc/relay:dev`
prints the usage line and exits 2, and the compose file passes the whole command including
the role.

| Role | What the room is |
| --- | --- |
| `viewer` | A read-only broadcast room |
| `commenter` | A review link |
| `suggester` | Suggestion mode |
| `editor` | A room people write in — what `docker-compose.yml` names |
| `owner` | What an unauthenticated relay used to be, now said out loud |

**This binary verifies no grants, so it cannot tell two participants apart.** Every
participant in the room gets exactly the role on the command line; the role is a ceiling
over the room, not an identity. `server/src/main.rs` states it directly, and `docs/143`
§16 Q5 records why no signature profile is built in. A deployment that needs
per-participant access implements `GrantVerifier` and passes `Access::Granted` — that is a
library integration, not a flag.

Read §8 before choosing `owner`.

### 5.4 What the relay refuses, by name

- **A full room.** The relay enforces a per-room participant ceiling of `MAX_PARTICIPANTS`
  (128, in `crates/casual-doc-transaction/src/presence.rs`) at admission, and refuses past
  it with `ODC-7010` / `collaboration_room_full` rather than letting the roster grow without
  limit. The check sits **after** the grant, so a caller with no grant cannot use the
  refusal to measure how full a room it was never admitted to is, and **before** the
  admission is journalled, so a durable record never describes a participant who was then
  refused.
- **A participant that cannot be written to.** `docs/152` §2c's back-pressure policy
  **evicts** a persistently unreachable participant from the room rather than letting every
  later chunk be written to a dead socket. The eviction is logged by name to stderr —
  "participant N could not be written to and was removed from the room; it resumes to catch
  up" — and the participant's next connection is readmitted. An eviction in the log is
  normal operation for a closed laptop, not an error to page on.

---

## 6. The journal: persistence, backup, restore

### 6.1 What it is

The journal is the room's **durable ordered log**, and it is the only copy of the order.
`Room::open` replays it rather than starting empty, "because starting empty would silently
drop every chunk the relay had already acknowledged". Losing it loses the room's history.

It is a single file. Both images put it on a **named volume** at
`/var/lib/opendoc-relay/room.journal`, and the volume holds the **directory**, not the
file, because `Journal::compact` writes a fresh journal beside the old one and renames it
into place — which needs the containing directory writable by the same user.

`docker compose down` keeps the volume. Only `docker compose down --volumes` destroys it.
That asymmetry is the point.

### 6.2 Backup

The relay opens the journal in append mode and `fsync`s once per ordered chunk, which is
"the whole of what durability costs". A copy taken while the relay is running is a copy of
a prefix of an append-only log — consistent up to some chunk, never torn in the middle of
one, because the recovery scan treats a partial final frame as the expected shape of a
crash between appends and keeps everything before it.

```sh
# A consistent copy without stopping the room.
docker compose run --rm --entrypoint sh relay \
  -c 'cat /var/lib/opendoc-relay/room.journal' > room.journal.bak
```

### 6.3 Verify the backup with `inspect`, which is what it is for

`inspect` exists "because a durable log that cannot be read without starting a server is a
log nobody will check". It replays the journal and reports what it holds:

```sh
docker compose run --rm relay inspect /var/lib/opendoc-relay/room.journal
# replayed N ordered chunks
# ordered entries retained: M
```

**Two operational facts, both from the code rather than from habit:**

1. **Stop the relay first, or inspect a copy.** `Journal::open` reopens the file in
   **append mode** — so `inspect` against a live journal is a second writer on a
   single-writer append-only log. It is a `run` invocation rather than a service precisely
   so that no `up`, with or without a profile, can ever start it (§1a).
2. **A non-zero `discarded … bytes of a partial final frame` line is information, not an
   error.** It is reported rather than hidden because it is "the only evidence the previous
   process did not shut down cleanly".

To verify a backup, inspect the copy:

```sh
docker run --rm -v "$PWD:/backup:ro" -v opendoc-journal-check:/var/lib/opendoc-relay \
  --entrypoint sh opendoc/relay:dev \
  -c 'cp /backup/room.journal.bak /var/lib/opendoc-relay/room.journal \
      && opendoc-relay inspect /var/lib/opendoc-relay/room.journal'
```

A restore that `inspect` cannot replay is a restore that `serve` will also refuse: a
corrupt or disagreeing journal is an error rather than an empty room, deliberately.

### 6.4 Restore

Stop the relay, put the file back at `/var/lib/opendoc-relay/room.journal` with the
journal directory owned by uid 10001, start the relay, and read the first two lines of its
log — `serve` prints the same recovery report `inspect` does before it binds the port.
**Do not restore over a live journal**, and do not run `create` to "initialise" a volume
that already holds one: it will refuse, which is the correct outcome.

---

## 7. TLS, and the reverse proxy that terminates it

**The relay does not speak TLS, and nothing in these images adds it.** The relay binds a
plain `std::net::TcpListener` and speaks a length-prefixed binary frame protocol; there is
no `rustls` and no `openssl` anywhere in `cargo tree -p opendoc-relay`, which is eleven
pure-Rust crates. **The editor image does not speak TLS either** — nginx listens on plain
`8080`.

So TLS is the deployment's job, and the shape is the usual one: terminate at a reverse
proxy and keep both containers on an internal network.

- **The editor** is ordinary HTTPS reverse proxying: any proxy that can serve static files
  over TLS will do, with the two rules from §3.3 — `.wasm` as `application/wasm`, and no
  rewriting of font URLs.
- **The relay** is **not HTTP**. It is a raw TCP frame protocol, so it needs **TCP/TLS
  passthrough or TLS termination at layer 4**, not an HTTP reverse proxy: nginx's `stream`
  module, HAProxy in TCP mode, or `stunnel`. An HTTP `proxy_pass` in front of it will not
  work, and this is the single most likely deployment mistake.

There is no authentication in front of either. The relay's role argument is a ceiling over
the whole room, not a login, and §8 says what follows from that.

---

## 8. What these deployments do not do

Stated here, in one list, because the alternative is an operator discovering it.

1. **No TLS in either process.** §7.
2. **No authentication, and no per-participant access.** The shipped binary verifies no
   grants; everyone who can reach the port gets the room's role. An `owner` room reachable
   from a network is a document anyone on that network can edit and whose history they can
   rewrite. Put a layer-4 proxy with an allowlist, a VPN, or a `GrantVerifier`
   implementation in front of it.
3. **No horizontal scaling, no clustering, no shared state between relays.** A room is one
   process and one journal file. Two relays on one journal would be two writers on one
   append-only log. There is no sharding, no leader election and no replication.
4. **No rate limiting and no request quotas.** The frame codec bounds a frame's size and
   presence bounds a payload to `MAX_PRESENCE_BYTES`, and the room bounds participants to
   128 — but nothing limits how often an admitted participant may submit.
5. **No metrics endpoint, no structured logs, no tracing.** The relay prints human lines to
   stdout and stderr. §10 is what you can actually monitor.
6. **No document in the relay, so no server-side export, conversion, PDF or search.** Those
   all live in the editor, in WebAssembly, in the tab.
7. **No CI job builds these images.** This document does not claim one. `SKILL.md` §9
   requires that prose describing a gate name the workflow and a test assert the gate is
   armed; there is no such workflow, so the images are built and run by hand and
   `webapp/tests/deployment_contract.test.mjs` guards only the contract between the compose
   file, the Dockerfiles, this document and `server/src/main.rs`. Adding an image-build job
   is a follow-up (§9).

---

## 9. Configuration surface, and the follow-ups it implies

### 9.1 Today

| Knob | Where | Note |
| --- | --- | --- |
| Editor host port | `docker-compose.yml` `ports` | Container side is fixed at 8080 by the nginx config |
| Editor caching, gzip, headers | `deploy/editor-nginx.conf` | Rebuild not needed if mounted |
| Journal path | `relay` `command` argument 2 | Must be inside the volume |
| Listen address | `relay` `command` argument 3 | `0.0.0.0:7070`; use `127.0.0.1` only with host networking |
| Room role | `relay` `command` argument 4 | The room's ceiling; §5.3 |
| Journal durability | Named volume `opendoc-relay-journal` | §6 |

### 9.2 Env-var configuration for the relay — the exact change

Deliberately **not** implemented here: `server/src/**` is owned by the co-editing lane, and
a compose file that implied env-var support the binary does not have is the kind of false
published claim §9 of `SKILL.md` exists to stop.

What it would take, concretely, in `server/src/main.rs`: `main()` collects
`std::env::args().skip(1)` into a `Vec<String>` and matches the slice against three literal
patterns. Supporting the environment means giving each positional a fallback *before* that
match — read `OPENDOC_RELAY_JOURNAL`, `OPENDOC_RELAY_ADDR` and `OPENDOC_RELAY_ROLE` and
substitute them where an argument is absent — and the one decision that is not mechanical
is whether a missing role may come from the environment at all. It may: an operator setting
`OPENDOC_RELAY_ROLE=viewer` has named a permission as deliberately as one typing it. What
must **not** happen is a default when neither is present, because that is the exact
decision `open_room_role` returning `None` currently refuses to take.

### 9.3 Other follow-ups this work identified

- **A CI job that builds both images.** Would catch a Dockerfile that stops building, which
  the contract guard cannot see. Not added, and not claimed (§8.7).
- **An unclean shutdown can make a room unopenable.** Found by reading
  `server/src/journal.rs` for §6, and it is a durability defect rather than a deployment
  one: `Journal::open` reports a partial final frame as `discarded_tail_bytes` but leaves
  those bytes on disk and reopens the file with `OpenOptions::new().append(true)`. The next
  ordered chunk is therefore appended **after** the partial frame, which turns a recoverable
  tail into mid-file corruption — and `scan` treats a short frame as recoverable only at the
  end of the file, so the following recovery returns `JournalError::Corrupt` and the room
  will not open. Filed as `109` HF-261 for the lane that owns `server/src`; nothing in this
  document works around it, and the mitigation until it is fixed is §6.2's backup.

---

## 10. Monitoring, honestly

There is no metrics endpoint. What exists:

| Signal | How | What it means |
| --- | --- | --- |
| Editor serving | `HEALTHCHECK` fetches `/editor.html`; `/healthz` returns 200 | A browser asking for the editor gets the editor |
| Relay listening | `HEALTHCHECK` opens a TCP connection to 7070 | The listener is bound and accepting |
| Recovery report | First lines of the relay's stdout on start | How many chunks replayed, and whether the last run shut down cleanly |
| Evictions | stderr, `participant N could not be written to` | Back-pressure removed an unreachable participant (§5.4) |
| Room full | A client receives `ODC-7010` | The 128-participant ceiling was reached |
| Participant errors | stderr, `participant ended: …` | A connection ended on a refused or malformed frame |

**What the relay healthcheck does not prove**, because overstating it would be the same
defect in miniature: the relay is thread-per-connection and the kernel completes a
handshake into the accept backlog, so a bound listener whose accept loop is wedged still
answers a connect. It is a liveness check on the socket, not on the ordering lock. The
connect-and-close is handled cleanly by the server — `participant()` reads one frame, sees
`ReadError::Closed` and returns — so it costs one short-lived thread and never occupies a
participant slot.

A room with no participants is indistinguishable from a healthy idle room, which is correct:
there is nothing to report.

---

## 11. Resource sizing, measured against the architecture rather than guessed

**The relay is `std`-only and thread-per-connection.** `serve` binds a `TcpListener`, and
for each accepted connection spawns a thread that loops on frames; `server/src/transport.rs`
records what that costs and why "a dumb relay is the one shape it suits". Every decision —
decide, journal, answer, fan out — is taken under **one mutex** held across all four,
"because two participants must not be told about the order in two different orders". There
is no async runtime, no thread pool and no work stealing.

What follows, stated as architecture rather than as a benchmark this document does not have:

- **Concurrency is one OS thread per connected participant**, plus the accept loop.
- **The room's ceiling is 128 participants** (`MAX_PARTICIPANTS`), refused by name past
  that. So one room is **tens of participants, not thousands** — and the constant's own
  comment says the same thing: "Word's own co-authoring tops out well below this, and a
  document with more readers than this wants a broadcast mode rather than a roster."
- **Fan-out is the cost that grows.** The roster is sent to everyone, so "the cost of the
  *n*-th participant is paid *n* times" — the ceiling is there because of that, not because
  of memory.
- **A lone writer pays almost nothing.** `docs/152` §10 Q3 measured the contention cost at
  exactly `(W − 1) / 2` wasted round trips per chunk that lands, so at `W = 1` it is one
  round trip per chunk plus one `fsync`.
- **Disk grows with history, and compaction is the answer.** The journal is append-only
  between checkpoints; `Journal::compact` rewrites it as one checkpoint and discards the
  replayed tail.
- **CPU per chunk is ordering and a frame copy** — the relay runs no transform and
  understands nothing about what an operation means (ADR-047).

**Sizing the editor is sizing a static file server**, and the honest number is the one from
the image: a few hundred KB of HTML and CSS, the WebAssembly module, the locale catalogues
and the dictionaries. Memory and CPU are the **browser's**, on the reader's machine, which
is the whole point and also where the real ceilings are — `SKILL.md` §8 and `docs/107` §4
own those, not this document.

**No number in this section is a throughput claim**, because no benchmark of the relay
under concurrent participants exists. When one does, it belongs here with its artefact.

---

## 12. Upgrading without losing a journal

The journal format is read by `Journal::open`, which **verifies** rather than trusts: it
re-takes every logged decision and compares, and a disagreement is
`JournalError::DecisionDiffers` rather than a silent divergence. So an upgrade either
replays the existing journal exactly or refuses to start — there is no path where a new
binary quietly reinterprets an old log.

```sh
# 1. Back up first, and verify the backup (§6.2, §6.3). Not optional: an upgrade that
#    refuses to replay leaves you needing exactly this file.
# 2. Stop the relay only. The volume is untouched by `down` without `--volumes`.
docker compose --profile relay stop relay
# 3. Rebuild the image from the new checkout.
docker compose --profile relay build relay
# 4. Start it, and read the recovery report in the first lines of the log.
docker compose --profile relay up -d relay
docker compose --profile relay logs relay | head -5
```

**Do not run `create` as part of an upgrade.** The volume already holds a journal and
`create` will refuse — correctly. The only time `create` runs is once, for a new room.

**If the new binary refuses the journal**, that is the guard working: it means the replayed
decision differs from the logged one, and the right move is to go back to the previous image
and report the journal (not the error message) to the lane that owns `server/src`. Starting
empty to get past it is the one thing that loses the room.

Upgrading the **editor** has no durable state to lose at all: rebuild the image and replace
the container. The HTML is served `no-cache` (`deploy/editor-nginx.conf`) precisely so a
reader does not keep a stale page pointing at a module that no longer matches it.

---

## 13. Embedding, and why there is no second embedding guide here

There already is one, it is generated, and `SKILL.md` §9 plus `docs/126` bind it to its
sources: `webapp/tools/build-embed-docs.mjs` generates every code panel, table and number
on `webapp/embedding.page.html` from the code they document, and `build.sh --check` fails
the build when the committed page is not what a fresh run produces. A second embedding
guide written in this document would drift from that one, which is exactly the failure this
repository keeps fixing.

So the container section lives in the **generator**, where its code panels are extracted
verbatim from the committed `Dockerfile.editor` and `docker-compose.yml` rather than
retyped, and `webapp/tests/embedding_page.test.mjs` proves each panel still appears in its
source file. What a host embedding the editor needs from this document is only §1 and §3:
the editor is static files, so embedding it is pointing the
element's `editor-src` attribute at an origin that serves this image, and that origin needs
no relay.

The host contract itself — the nine capabilities, the five roles, the commands, the events,
the refusal codes and the origin allowlist — is `webapp/src/host_contract.mjs` and
`packages/opendoc-embed/`, documented on that page. Nothing in it changes because the files
are served from a container.

---

## 14. Why each shape was chosen

Named prior art first, per `SKILL.md` §8, because none of this is new.

| Decision | The established pattern | Why here |
| --- | --- | --- |
| Multi-stage build | Builder/runtime separation | No toolchain in the runtime image; the relay runtime is a binary and the editor's is a directory |
| Allowlist `.dockerignore` | Default-deny | A deny list ships what it forgot, and this tree holds the owner's untracked personal documents beside the source |
| Named volume for the journal | Durable state outside the container lifecycle | The journal is the only copy of the room's order |
| Compose profile for the relay | Opt-in service | "No mandatory server" has to be true of the compose file too, or the file teaches the opposite |
| One service per process, and two images rather than one | One concern per container | A merged image is +119% on the single-user default to carry a server it never runs, or a musl rebuild that still puts two processes under one PID 1 — §3.5 has the measurements |
| `run` for the one-shot commands | A job, not a service | A service that exits is never up: it is noise in `docker compose ps` and an invitation to `up` it (§1a) |
| No `CMD` in the relay image | Fail closed | A default role would be the permission the binary refuses to choose |
| Non-root, read-only root, tmpfs | Least privilege | The editor writes nothing; the relay writes one directory it owns |
| Healthchecks that connect | Readiness over liveness | "The process exists" is not "the port is serving" |
| Layer-4 proxy for the relay | TLS termination at the edge | The relay is not HTTP, so an HTTP proxy cannot front it |

---

## 15. Guards

`webapp/tests/deployment_contract.test.mjs`, in the Node unit suite
(`npm run test:unit`), parses `server/src/main.rs`, `docker-compose.yml`, both Dockerfiles,
the nginx config, `rust-toolchain.toml`, `.github/workflows/ci.yml`,
`crates/casual-doc-transaction/src/presence.rs` and this document, and asserts:

1. **The subcommands and argument order match the binary.** Every `command:` in the compose
   file is one of the three patterns `main()` actually matches, with the same arity and the
   same positions — so a CLI change breaks the build instead of shipping a compose file that
   cannot start.
2. **The roles are exactly the five the binary accepts.** The set named in the compose file
   and in §5.3 of this document equals the set `open_room_role` matches; a sixth role in
   either, or a fifth that was removed, fails.
3. **The ports agree.** The port this document states equals the port compose publishes,
   equals the address the `serve` command binds, equals `EXPOSE` in `Dockerfile.relay`; and
   the editor's container port equals what nginx listens on.
4. **The pinned versions agree with their sources** — Rust against `rust-toolchain.toml`,
   wasm-pack and the Node major against the CI workflow.
5. **The invariants that are easy to erode**: the relay image declares no `CMD` and a
   non-root `USER`, the journal volume path contains the journal the compose command names,
   and the nginx config contains no `sub_filter` and no Google-Fonts host.
6. **The participant ceiling in this document is the constant**, not a number typed here.
7. **No compose service is a one-shot command.** Every `command:` in the file must be the
   subcommand that binds an address — derived from `main.rs`'s own bindings, not from the
   name `serve` — and no service may declare `restart: "no"`. This is what keeps `create`
   and `inspect` out of the file (§1a).
8. **The default `up` set is exactly `[editor]`**, and §2's two new counts are the file's.
9. **Every relay invocation this document publishes is one the binary matches.** The
   runnable ```sh lines are parsed for `docker compose run … <service> <subcommand>` and
   for `opendoc-relay <subcommand>`, and each is checked for the subcommand, the arity, the
   argument ORDER by shape, and a journal path inside the declared `VOLUME` — the same
   assertions a compose `command:` gets. And the coverage statement that makes removing the
   one-shot services safe: **every subcommand no compose service runs must have a runnable
   example here**, so deleting one of the two examples fails the build.
10. **The embedding page's deployment panel is derived from the binary too.** The generator
    reads `server/src/main.rs` for the subcommand names, and the committed page may not
    name a `relay-create`/`relay-inspect` service.

It was driven red before it was trusted; the mutations and their output are in the commit
message.

**One thing in this document is NOT guard-derived, and is labelled rather than left to be
assumed:** the five image and binary sizes in §3.5. They are `docker image inspect` and
`ls -l` readings taken on an arm64 machine, and no test can re-derive them without a Docker
daemon — the unit suite has none, and §8.7 records that no CI job builds these images. They
are reported with the command that produced them and the architecture they were measured
on, and they will drift as the images change; nothing downstream computes anything from
them.

---

## 16. What this does not include

- **No Kubernetes manifests, no Helm chart.** The compose file is the reference deployment;
  a chart that nobody runs is a chart that rots.
- **No published images.** Nothing pushes to a registry, so every example above builds
  locally. Tags read `:dev` for that reason.
- **No multi-architecture build.** The Dockerfiles detect `amd64` and `arm64` for the
  wasm-pack download and are otherwise architecture-neutral, but no `buildx` manifest list
  is produced.
- **No desktop packaging.** `109` RM-12 owns that, and it is deliberately after the
  browser-first work.
