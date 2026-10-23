# 175 — Host identity and directory: who is here, who wrote this, who can be mentioned

**Status:** Design accepted 2026-10-23 (owner decisions in §12). **Built by HF-360 (PRs #881 and #883)
except §7 (the @mention directory, `docs/109` HF-361) and §11 (shared version history,
HF-362)**, whose seams are kept. §15 records where the build differs from this text and why.
**Decides:** ADR-070, **identity is attested by the host and attached at the order. A
participant never states who wrote an operation.**
**Rows:** `docs/109` HF-360, for findings `docs/174` RC-05, RC-06, RC-07, RC-09 and RC-10.
UX-050 keeps RC-09's history and its other rows. RM-45 (`docs/168` batch 30, "Mentions and
comment ownership") gets its identity half from this contract.
**Builds on:** `152` (protocol, §2b presence, §4 id spaces, §10 Q4 and ADR-060 grants),
`169` (Share provider, presence, recovery; ADR-067), `149` (ONLYOFFICE option map),
`126` and ADR-036 (one host contract, two transports), `68` §1 (the `setActiveAuthor`
seam), and ADR-038 (local version history).

---

## 1. What is wrong today, measured from the code at `04be0737`

Every claim below was read in the file it cites.

| # | Fact | Where | Consequence |
| --- | --- | --- | --- |
| F1 | The identity a shared session joins with is the Settings author name, or the literal `"You"`. | `webapp/src/collab_shell.mjs:32` | **Self-asserted.** Anyone can type anyone's name. Every unnamed participant joins as the same identity, `"You"`. |
| F2 | The engine's join frame always sends `grant: None`. | `crates/casual-doc-wasm/src/collab.rs:742` | A relay running `Access::Granted` is **unreachable from the browser**. ADR-060's verified path exists only in Rust tests. |
| F3 | `GrantVerifier::verify` returns `Capabilities` only. | `server/src/access.rs:108` | The verifier cannot say whom a grant is for. Matching the grant's subject to the join is documented as optional: "if the deployment cares", `access.rs:89–93`. |
| F4 | A remote caret is labelled `t("collab.participant", {number})`, and its colour is `hsl(client × 137.508)`. | `webapp/src/collab_presence.mjs:55` and `:45` | RC-06. The roster carries no name. `ServerMessage::Awareness` has `client` and nothing else. |
| F5 | Comment cards render `author \|\| initials \|\| "You"`. The engine is handed `authorName \|\| "You"` as the active author. | `webapp/src/main.js:1931`, `:2157`, `:2177`, `:11169` and `:15729` | RC-05, and it is worse than the finding says. The **string `"You"` is written into the document** as `w:author`. Every other reader, including Word, then sees a person called "You". |
| F6 | Comments and revision marks travel inside `Operation::UpdateReviewState`, which carries the paragraphs it touched and an optional **replacement of the whole comments map**. | `crates/casual-doc-edit/src/lib.rs:774` | Authorship sits inside a payload. The relay holds no document (ADR-047), so it cannot tell a new comment from an existing one. |
| F7 | `people.xml` is already modelled and round-tripped: `Person { author, presence: Option<PresenceInfo { provider_id, user_id }> }`, and `Comment::person` joins on the author name. | `crates/casual-doc-model/src/v1/definitions.rs:1018–1050` | The DOCX carrier for a stable id **already exists**. Nothing writes to it. |
| F8 | `setActiveAuthor(name, initials, id)` takes an opaque `id` and "never serializes it into the document". | `crates/casual-doc-wasm/src/lib.rs:11048–11051`, `68` §1 | The engine seam exists, and it is the right one to extend. |
| F9 | `prefs={"user":…}` is a **preference**: "a visitor's own stored choice still wins". | `webapp/src/capabilities.mjs:452`, `149` §"three axes" | A host that knows who is looking **cannot pin the name**. ONLYOFFICE's `editorConfig.user.name` is lowered onto a value the visitor can overwrite. |
| F10 | Version rows already carry an `actor`, taken from `settings.authorName`, and are stored in the browser's `opendoc-drafts` IndexedDB. | `webapp/src/version_history.mjs:920`, `main.js:15698` | RC-09. The field exists. The value is usually empty, and the store is per browser. |
| F11 | The relay journals `Admission { client, capabilities, resume: (ResumeKey, Identity) }`. | `crates/casual-doc-transaction/src/session.rs:1159` | The opaque `Identity` is **persisted on the relay's disk** today, so it must not be an email address. |
| F12 | `@` in a comment does nothing special. | `docs/174` RC-10; `grep mention webapp/src` finds nothing | There is no directory seam. |

The root cause is one defect, not five. **There is no attested identity anywhere between
the host and the pixels.** The relay numbers connections, the chrome invents a label, and
review stamps whatever Settings holds. The findings are its symptoms: RC-05 (author),
RC-06 (label), RC-07 (roster), RC-09 (version author) and RC-10 (directory).

---

## 2. The patterns, named before any design

| Pattern | Where it applies here | Prior art |
| --- | --- | --- |
| **Claims inside a signed token** (bearer assertion, RFC 7519 shape; the encoding stays open) | §4: the profile travels inside the grant the host signs, next to the capabilities. | ADR-060 already does this for capabilities. `GrantToken`'s own doc comment says "the claims travel inside the signed blob". |
| **Server-attached attribution**: the authenticated session names the actor, and the mutation does not | §5: the relay attaches the author to every `Arrival`, as it already attaches `client` to `Awareness`. | `152` §2b. ONLYOFFICE's received cursor carries `user` and `useridoriginal`, which its sender never states. Google Docs attributes edits from the signed-in account, not from the mutation body. |
| **Normalise on apply**: a deterministic function every replica runs | §5.2: `authorship::conform` restamps the review content an arrival introduces. | The same reason `152` §4.2 derives the minting space from `client` and does not carry it: "no wire field to forge". |
| **Snapshot then deltas** for a membership list | §4.3: the full list on `Welcome`, then `ParticipantJoined` and `ParticipantLeft`. | Yjs awareness. ONLYOFFICE `_onParticipantsChanged` replaces the list wholesale. Deltas are chosen here because of the 128-participant ceiling (§4.4). |
| **Typeahead over a remote index**: query-driven, bounded, cancellable, never enumerated | §7: the `@` directory. | Word and Google Docs people pickers. Contrast ONLYOFFICE `onRequestUsers`, which asks the host for the list and filters it on the client (§9). |
| **Opt-in remote resource loading** | §8.1: avatars. | E-mail clients block remote images by default, because a fetched image tells its server who looked and when. |

No new mechanism is invented. Every piece is an existing seam in this repository extended
by one field, or a named pattern applied once.

---

## 3. The identity object: one shape for every transport

```ts
interface HostIdentity {        // version 1
  id: string;                   // opaque, stable per person, 1..128 bytes. NEVER shown.
  name: string;                 // display name, 1..255 bytes (the w:author bound, F7)
  initials?: string;            // 1..8 graphemes. Default: derived from name, as setActiveAuthor does
  color?: string;               // "#rrggbb". Default: derived from id, never from name
  avatar?: { url: string } | null;  // never fetched without consent (§8.1)
  persist?: {                   // OPT-IN: what may be written into a saved file (§6.2)
    providerId: string;         // 1..255 bytes, e.g. "AD" or the host's own name
    userId: string;             // 1..255 bytes; the host's id for this person in that provider
  };
}
```

Rules on the shape:

- **`id` is pseudonymous by contract.** The relay journals the join identity (F11), and the
  id is fanned out to every participant. A host must not use an e-mail address or a
  government id. The editor cannot check that, so the contract says it and the reference
  host shows a hash.
- **Bounds are refusals, not truncations**, as for every network value in `152` (§2b
  "Bounds"). The value is refused with `identity-invalid`, naming the field.
- **`id` is not `persist.userId`.** One travels on the wire and identifies a session's person.
  The other is written into a file that can leave the organisation. Merging them would make
  every saved DOCX carry the wire id, which `68` §1 rightly forbids today (F8).
- **No `email` field.** Nothing in this contract needs one. Where a host wants a mention to
  reach an inbox, that happens on the host's side of the directory seam (§7.3).
- **`group` is reserved, not accepted.** ONLYOFFICE's `user.group` exists only for
  `reviewGroups` and `commentGroups`, which this contract does not build. A supplied
  `group` is reported as `not-honoured`, the way `149` reports every option it cannot honour.

### 3.1 It is a fourth axis, not a preference

`149` keeps capabilities (narrow only), regions (composition) and preferences (defaults the
visitor may change) apart. Identity fits none of them:

- It is **not a preference**, because a visitor must not be able to override it. F9 is the
  hole this closes.
- It is **not a capability**, because it neither narrows nor widens anything. It names.
- It is **not composition**.

It is therefore a fourth input with one rule:

> **When a host supplies an identity, it is the only identity.** The Settings author name and
> initials fields are disabled, with the reason "Your name is set by the application you
> opened this in". The `prefs.user` and `prefs.initials` preferences are ignored and reported
> as `superseded-by-identity`.

When no identity is supplied, nothing changes from today's preference behaviour, apart from
the defect in §10.1.

### 3.2 The three transports, one schema (ADR-036)

| Transport | How the host says it | When it may change |
| --- | --- | --- |
| **Embed element** | `<opendoc-editor identity='{"id":"u_8f2c","name":"Ada Lovelace"}'>`, added to `MOUNT_ATTRIBUTES` (`embed_element.mjs:62`). Also `element.identity = {...}` as a property, for hosts that would rather not put a name in the DOM. | Boot only. A change remounts, as every other mount attribute does today. |
| **In-process API** | `mount(target, { identity, … })`, validated by the same function `host_contract.mjs` exports for the bridge. | Boot only. |
| **postMessage** | `session.identify { identity }`, accepted **exactly once**, after the frame's `ready` and before a document is opened or a room is joined. A second call is refused with `identity-fixed`. | Never after the first. |
| **ONLYOFFICE `config`** | `editorConfig.user.{id,name,image}` is lowered onto `identity.{id,name,avatar.url}` (not onto `prefs.user`, as it is today). `user.group` is reported `not-honoured`. | As above. |

**Never in a URL.** The iframe's first URL carries `mode`, `can`, `chrome` and `prefs`
(`149` §"Composition is decided once"). An identity there would land in browser history,
`Referer` headers and every access log between the host and the editor origin. That is why
the frame transport is a one-shot message and not a query parameter. The element moves
the attribute into that message itself, so a host using `<opendoc-editor>` never handles
postMessage.

**Why boot-only.** A mid-session identity change would put two people's work in one undo
history and one resume key (`152` §5.5 scopes the key to the identity). ONLYOFFICE also
needs a new editor instance for a different user. A host that switches users remounts.

### 3.3 The grant is not part of the identity, and it is never an attribute

The grant is a credential. The identity is display data. The grant reaches the editor only
through a provider call made **per join**, because the relay re-verifies on every join,
reconnects included (`152` §10 Q4):

```js
// added to globalThis.OPENDOC_COLLABORATION (169) and to the element's provider property
async credentialsFor({ room }) {
  return { grant: "<opaque bytes or base64url string, at most 8 KiB (MAX_GRANT_BYTES)>" };
}
```

`collabJoinFrame(identity, resumeKey, revision, grant?)` gains the fourth argument, which
closes F2. The grant is held only for the duration of the call. It is never stored, logged,
put in a URL, put in an attribute, or shown in a diagnostic.

---

## 4. Identity on the wire

### 4.1 Attested, not asserted

`GrantVerifier::verify` returns what the grant says about the person, not only what they may
do:

```rust
pub struct Verified {
    pub capabilities: Capabilities,
    pub subject: Identity,          // REQUIRED: whom this grant was issued to
    pub profile: Option<Profile>,   // display claims the host signed
}
fn verify(&self, token: &GrantToken) -> Result<Verified, GrantRefusal>;
```

The relay then applies one rule, and it is not optional:

> Under `Access::Granted`, **`join.identity` must equal `verified.subject`**, or the join is
> `NotAuthorised` (`ODC-7003`). The profile fanned out is the **grant's**. Anything the client
> sent is ignored.

This turns `access.rs:89–93`'s "if the deployment cares" into a requirement. A grant for one
person presented with somebody else's identity is not a grant for either, and the relay now
refuses it instead of leaving that to each verifier.

`Profile` is the wire subset of §3: `id` (the subject), `name`, `initials`, `color` and
`avatar_url`. It never includes `persist`. That pair is file data and stays in the
participant's own editor (§6.2).

### 4.2 Open rooms: claimed, and labelled as claimed

`Access::Open` cannot tell two participants apart (`access.rs:115–123`). `Join` gains
`#[serde(default)] profile: Option<Profile>`. **It is read only under `Access::Open`**, and
the relay fans it out marked `attested: false`. In an open room:

- the name is shown, with an "unverified" mark in the people list and in the caret tooltip;
- the colour is the relay's palette, never the claimed one, so a claimant cannot copy
  somebody else's colour;
- **the avatar URL is dropped by the relay**. A claimed avatar is a tracking pixel aimed at
  every other participant (§8.1).

### 4.3 Who is here: a participant list for everyone

Today only a participant with `manage_access` receives `Member`s (`protocol.rs`, `Welcome`).
That list carries capabilities, which is the room's policy, so it rightly stays restricted.
**Who is present is not policy.** Every participant gets:

```rust
pub struct Participant { client: ClientId, profile: Option<Profile>, attested: bool }

ServerMessage::Welcome  { …, participants_present: Vec<Participant>, me: Option<Profile> }
ServerMessage::Resumed  { …, participants_present: Vec<Participant>, me: Option<Profile> }
ServerMessage::ParticipantJoined(Participant)
ServerMessage::ParticipantLeft { client: ClientId }
```

`me` is the profile the relay will attach to this participant's arrivals. The client stamps
its own optimistic review content with exactly that value (§5.2), so an honest replica's
optimistic state equals what everybody else computes.

Two new `ServerMessage` variants are a hard break (`152` §2b: an unknown enum tag does not
deserialize). **`PROTOCOL_VERSION` 6 → 7.**

### 4.4 Bounds and cost

| Bound | Value | Why |
| --- | --- | --- |
| `MAX_PROFILE_BYTES` | 2 KiB encoded | A name of at most 255 bytes, initials, a colour and a URL of at most 1,024 bytes fit with room to spare. |
| Avatar URL | at most 1,024 bytes; `https:` only, or `data:image/{png,jpeg,webp}` of at most 1,024 bytes | No `http:`, no `javascript:`, no credentials in the URL. |
| Participants | `MAX_PARTICIPANTS` = 128, unchanged (`152` §2c) | — |

Cost: a join fans out one `ParticipantJoined` (at most 2 KiB) to *n* − 1 participants, and the
joiner's `Welcome` carries at most *n* × 2 KiB, which is at most 256 KiB at the ceiling. Per
keystroke the cost is **zero**: profiles are never sent with presence, only on membership
change. The `Arrival.author` stamp in §5 is at most 2 KiB per *chunk*, not per keystroke,
because coalescing (`107` §4 B3) makes one chunk per typing run.

### 4.5 What the relay stores, and what it logs

- **Kept in memory only:** the profile of each open connection. It is dropped on disconnect.
  It is not presence, but like presence (`152` §2b) it is not durable state.
- **Journalled:** the `Arrival.author` stamp (§5.1), because a late joiner replays missed
  arrivals and must stamp them the same way. This adds no new class of personal data: the
  author name is already inside every review operation the journal holds today.
- **Logged:** participant numbers, room ids and refusal codes. **Never** names, ids, avatar
  URLs or grant bytes. `GrantRefusal`'s operator detail stays as it is.

---

## 5. Authorship: attached at the order, conformed on apply

### 5.1 Why not "check the author field"

The obvious design, a relay that refuses an operation whose author is not the sender, does
not work with the operation set we have. F6: a comment arrives inside an `UpdateReviewState`
whose `comments` is a **replacement of the whole map**, and the relay holds no document, so it
cannot know which entries are new. A replica-side refusal is worse. The relay has already
ordered the chunk, so a refusing replica is one ordered revision short, which is the
divergence `169` §"Known limit" records for `IdCollision`.

### 5.2 The decision

1. **The relay attaches the author.** `Arrival` gains `author: Option<Profile>`, taken from
   the connection's attested (or, in an open room, claimed) profile. The client's
   `Submission` gains no field. Claiming another author is unexpressible, which is the
   `Awareness` argument (`152` §2b) applied to edits.
2. **Every replica conforms on apply.** `casual_doc_edit::authorship::conform(document,
   operations, author)` runs inside the same choke point as `localise` (`152` §4.2). It is a
   deterministic function of the document before the arrival, the operations and the
   stamp:
   - a comment whose id **was not in the document** takes the stamp's `name` and `initials`,
     and its `person` is set to that name;
   - a revision mark that the operation **introduces** takes the stamp's `name`. A mark is
     introduced when its paragraph's projection did not contain it, which is ADR-052's
     projection comparison. It is O(touched paragraphs), because `UpdateReviewState`
     carries only touched paragraphs;
   - a comment or mark that **already existed** keeps the author it had. An operation that
     tries to rewrite another person's authorship has that part restored, and the rest of
     the operation still applies;
   - the **date** of an introduced item is kept as authored. It is display data, Word lets
     it be anything, and a relay clock would be one more value to trust.
3. **The author's own replica conforms at commit**, with `me` from `Welcome`. An honest
   participant's optimistic state is therefore already conformed, the function is a no-op on
   arrival, and nothing rolls back.

Every replica runs the same function on the same ordered input, so **convergence is kept and
nothing is refused**. A rewritten client that forges an author sees its forgery only on its
own screen, until its next recovery. Everyone else, and every saved file, shows the attested
author.

What this deliberately does not do: authorship of content **pasted** with its marks (a pasted
tracked change keeps its original author in Word). Conform touches only marks the operation
introduces, and a carried subtree's marks are introduced. Pasted tracked changes in a shared
room are therefore restamped to the paster. **That differs from Word, on purpose**, until
`152` §10 Q2's subtree walk exists. The code must say so where it does it.

### 5.3 Standalone mode

There is no relay and no stamp. The active author is the host identity if one was given,
otherwise the Settings name, otherwise the anonymous value in §10.1. `conform` does not run.

### 5.4 "You"

A comment card or mark reads "You" **only when its author is this participant**. That means
the person's `persist.userId` matches when the mark's `Person` carries one, and otherwise
the name matches `me.name` (in a room) or the active author name (standalone). Every other
card shows the stored author. The word "You" is a rendering. **It is never written into a
document** (§10.1).

---

## 6. What the DOCX stores, and how it round-trips

### 6.1 Per mark

| Item | Attribute | Value |
| --- | --- | --- |
| Comment | `w:author`, `w:initials`, `w:date` | display name, initials, authored date |
| Insert, delete, move, format and property change | `w:author`, `w:date` | display name, date (OOXML gives revision marks no initials) |
| Core properties | `cp:lastModifiedBy` | the display name of the last author who saved, as Word does |

This matches Word, and it means the name is **frozen at authoring time**. A person renamed
later keeps their old name on their old marks, exactly as in Word.

### 6.2 The stable id: `word/people.xml`, opt-in

Word's own carrier for "which person is this author" is `w15:person` with
`w15:presenceInfo w15:providerId w15:userId`, and Word writes it for signed-in authors. The
model already holds it (F7). The rule:

- Every author the editor stamps gets a `w15:person w15:author="<name>"` entry.
- **`w15:presenceInfo` is written only if the host supplied `identity.persist`**, with the
  host's `providerId` and `userId` verbatim. Without `persist`, no id of any kind enters the
  file. That is today's `68` §1 guarantee, kept as the default.
- An imported `people.xml` entry is preserved verbatim (verbatim retention, `SKILL` §1).

**The known limitation, which comes from OOXML and not from us.** `people.xml` joins to marks
**by author name**. Two different people with one display name are one person once the file
is saved, in Word as well. The editor detects it when it would write two `Person` entries
with one `author` and different `userId`s. It writes the first, reports
`identity-name-collision` to the host (event `error`, never silent), and leaves the
disambiguation to the host, which knows who these people are. Whether the editor should
instead suffix the second name is §12 Q3.

### 6.3 Round-trip

Open, then save, preserves `w:author`, `w:initials`, `w:date` and `people.xml` byte for byte
(existing behaviour). A file authored here and opened in Word shows the attested names, and,
with `persist`, Word's presence lookup for `providerId="AD"` works when the host used Entra
ids. A file opened by a different host keeps the names and, unless the providers match,
ignores the ids. Nothing is lost and nothing is reinterpreted.

---

## 7. The directory seam for @mention

### 7.1 The rule

> **The editor never enumerates people.** It asks the host one bounded question, "who
> matches what the reader typed after `@`", and shows the answer. With no host directory,
> `@` is just a character.

### 7.2 The contract

```ts
interface Directory {
  search(query: string, opts: { limit: number; signal: AbortSignal }):
    Promise<{ people: DirectoryEntry[]; more: boolean }>;
  notify?(mention: MentionNotice): Promise<{ status: "notified" | "needs-access" | "declined" }>;
}
interface DirectoryEntry { id: string; name: string; initials?: string;
                           avatar?: { url: string } | null; link?: string /* e.g. mailto: */ }
interface MentionNotice { people: string[] /* entry ids */; commentId: string;
                          excerpt: string /* at most 280 characters of the comment */;
                          room?: string }
```

| Transport | Spelling |
| --- | --- |
| Element / in-process | `element.directory = { search, notify }` (functions cannot be attributes) |
| postMessage | event `directory-query { qid, query, limit }`, answered by the command `directory.answer { qid, people, more }`. Event `mention { … }` is answered by `mention.answer { qid, status }`. Both reuse ADR-036's correlation id. |
| Neither present | No picker, no fetch, no message. `@Ada` stays plain text. |

Behaviour:

- **Query only after one character.** A bare `@` sends nothing, so the host is never asked
  for "everyone". Debounce 150 ms. Each keystroke aborts the previous request. `limit`
  is at most 20. Results over the limit, or entries over §4.4's bounds, are dropped and the
  drop is reported, not silently trimmed.
- **Nothing is cached** past the open picker. Closing it discards the results.
- **The query is the only thing sent.** No document text, no title, nothing about other
  participants.
- **`notify` runs after the comment is ordered** (acknowledged in a room, saved in
  standalone), never on keystroke. An unacknowledged mention that is later lost must not
  have already e-mailed someone.
- `needs-access` is shown to the reader as the host's answer ("Ada can't open this
  document"). **Sharing is the host's product** (`149` declines `sharingSettingsUrl`), so the
  editor offers no share dialog. That matches what ONLYOFFICE's `mentionShare` implies, but
  the host decides it.

### 7.3 What a mention is in the document

The comment text holds `@Name`. When the entry has a `link`, the mention is a hyperlink run
to that link. This is the shape Word's mentions take on open, as far as the review lane has
seen. It is **not verified against a Word-authored fixture**. That check is the first task of
the build (§12 Q4), and the model shape is not frozen before it. Who was notified is not
stored in the document: that is host state.

---

## 8. Security and privacy

### 8.1 Avatars: no fetch without the host's consent

A URL the editor fetches tells that URL's server the reader's IP address, user agent and the
time they had the document open. A host-signed URL pointing at the host's own CDN is the
host's business. Any other URL is a tracking beacon. So:

- **Default: never fetched.** The chip shows initials on the identity colour.
- **The host consents per mount:** the attribute `avatars="fetch"` (or `{ avatars: "fetch" }`
  in-process, or ONLYOFFICE's `user.image` lowering when `avatars` is set). Only then are
  attested and directory avatar URLs loaded.
- **Even then:** `https:` only (§4.4), `<img referrerpolicy="no-referrer" decoding="async"
  loading="lazy">` with no `crossorigin` credentials, never a CSS `background-image` (which
  cannot set a referrer policy), and the host's own CSP `img-src` must allow the origin. A
  load failure falls back to initials, silently, because it is presentation.
- **Claimed (open-room) avatars are dropped at the relay** (§4.2), whatever the consent.
- `data:` avatars at most 1,024 bytes are allowed without consent, because they cause no
  fetch.

### 8.2 Personally identifiable data on the wire

| Datum | Who sees it | Stored where |
| --- | --- | --- |
| `id` (pseudonymous) | the relay; every participant in the room | relay journal (F11), relay memory |
| `name`, `initials`, `color` | every participant | relay memory; the `Arrival.author` stamp in the journal; the document |
| `avatar.url` | every participant (attested only) | relay memory only |
| `persist.userId` | **nobody**. Never on the wire. | the saved file, only if the host opted in |
| grant bytes | the relay's verifier | nowhere. Never logged. |
| directory queries and results | the host | nowhere in the editor |

The host decides what a name is. A host that must not reveal real names to co-participants
signs a pseudonym. Google Docs' "Anonymous Animal" is that choice made by the host, and
this contract lets a host make it.

### 8.3 Logs

The editor's console, `error` events and diagnostics name participants by number. They
never print a name, an id, a URL or a grant. A guard scans `collab_*.mjs` and the relay's
`tracing` calls for the profile fields, the way `presence_is_never_written_to_the_revision_log`
scans for presence types.

---

## 9. Comparison

### ONLYOFFICE Docs

From their published API documentation (`editorConfig`, `document.info`, `events`). These
are documented identifiers. This lane did not verify them against the checked-out source,
which is not present in this worktree. **No ONLYOFFICE code is reproduced.**

| Theirs | What it does | Ours |
| --- | --- | --- |
| `editorConfig.user.{id, name, image, group}` | Identity in the page config. It is trustworthy only when the whole config is JWT-signed. | §3. Signed claims are in the **grant**, and display data is not trusted on its own. `group` is reserved. |
| `user.image` | The editor fetches the URL directly. | Fetched only with consent (§8.1). |
| `document.info.sharingSettings` `[{user, permissions, isLink}]` | A read-only display of who has access. | Not a goal. The participant list (§4.3) shows who is **here**. Who has access is the host's product. |
| `events.onRequestUsers` with `setUsers` (`c: "mention"`, …) | The editor asks the host for a list, then filters it on the client. | A query per keystroke, bounded and never enumerated (§7). The host does not hand the client its directory. |
| `events.onRequestSendNotify` (`emails`, `actionLink`, `message`) | Notify after a mention. | `notify` after ordering, with entry ids, not e-mail addresses (§7.2). |
| `customization.mentionShare` | Offer to share with somebody mentioned. | `needs-access` comes from the host. The host shares. |
| Co-editing user list (header users button) | Shows names and avatars. | §4.3, which `docs/174` RC-07 is about. |

### Google Docs

Identity is the signed-in Google account. Names and photos come from the account
profile. Anonymous viewers get a pseudonymous animal. `@` searches the reader's contacts and
domain directory, and mentioning somebody without access prompts to share. Versions are
server-side, attributed per account, and shared by everyone. **Our differences:** there is
no account system of ours, so the host plays Google's role. Standalone editing with no
account exists, and Docs has no equivalent.

### Word co-authoring (Microsoft 365)

Identity is the Microsoft or Entra account. Presence shows profile names and photos. The
people picker queries the organisation directory and sends e-mail. Authorship is stored as
`w:author` display names, with `people.xml` `presenceInfo` (`providerId="AD"`) carrying the
account id, which is the shape §6.2 adopts. Offline Word uses the locally typed user name,
which is self-asserted, like our standalone mode. Word's Document Inspector anonymises
authors to "Author"; the owner kept "You" instead (§10.1).

---

## 10. With no host identity: what the reader gets, stated plainly

| Situation | Caret label | People list | Comment and mark author | Version author |
| --- | --- | --- | --- | --- |
| Standalone, Settings name set | — | — | the Settings name (self-asserted, as in Word) | the Settings name |
| Standalone, no name | — | — | **"You"** (§10.1, owner decision) | none |
| Room, open access, no profile claimed | **"Participant N"**, as today | numbered entries | unstamped (`Arrival.author` is `None`, so `conform` leaves it): the Settings name, else "You", which is self-asserted | the Settings name, else none |
| Room, open access, profile claimed | the claimed name, marked unverified | the claimed names, marked unverified | the claimed name, stamped by the relay | the claimed name |
| Room, granted | the attested name | the attested names | the attested name | attested by the host (§11) |

**"Participant N" stays** as the honest label for the case where nobody said who anyone is.
It is not a defect in that case, and it must not be dressed up as a name.

### 10.1 The anonymous author stays "You" (owner decision, 2026-10-23)

The draft proposed storing Word's anonymised value, "Author", instead of the literal
`"You"` as `w:author`. **The owner decided to keep "You"** (§12 Q1). So an unnamed author is
still stored as "You", and Word will show a person named "You". That is a known difference
from Word, recorded here so nobody "fixes" it back without the owner. What does change is
that a *named* or host-attested author is never replaced by "You": the "You" rendering rule
(§5.4) applies only when there is no name at all.

---

## 11. Shared version history: what needs the host, and what stays local

`169`'s provider gains three optional methods:

```js
async listVersions({ room, before, limit })  // → { versions: [{ id, revision, createdAt,
                                             //     author: { id, name }, name?, kind }], more }
async saveVersion({ room, revision, name, bytes })   // → { id }
async versionBytes({ room, id })                     // → { bytes, revision }
```

- **The host attests the author of a version from its own session.** `saveVersion` carries
  no author field. A version author the client could state is one it could forge, as with
  everything else here.
- **In a room with `listVersions`:** the timeline shows the room's versions, shared by everyone
  and authored. Local capture (ADR-038) continues underneath as this device's safety net,
  and its rows appear in a separate "On this device" group, never mixed with shared rows.
- **In a room without it:** today's panel, headed honestly "Versions on this device only".
  That is RC-09's current state, labelled instead of implied.
- **Standalone:** unchanged. IndexedDB, local-first, and `actor` is the active author name.
- **Restore in a room** stays refused (ADR-068) until HF-314. Shared history is listing,
  naming, viewing, comparing (ADR-064, ADR-065), copying and downloading, not restoring.
- Version bytes are the `collabSnapshot()` package of `169`, not a second format.

---

## 12. Owner decisions (2026-10-23), and what is still open

1. **Anonymous author: keep "You".** The owner chose the current behaviour over Word's
   "Author" (§10.1). A known difference from Word.
2. **Open rooms show claimed names, marked unverified** (§4.2), as recommended. Avatars are
   never shown for a claimed identity, and the relay assigns its colour.
3. **Duplicate display names get a suffix.** The editor shows the second person as
   "Ada Lovelace (2)" in carets, the people list and review labels. **The suffix is
   display-only:** the stored `w:author` keeps the name exactly as given, because altering a
   person's name inside a legal document is not the editor's to do. Identity is still kept
   apart internally by id. The host is also told, so it can fix the clash at the source.
4. **The DOCX shape of a mention** (§7.3) must be measured from a Word-authored file before
   the model is extended. This is a build task, not a preference.
5. **Pasted tracked changes in a room take the paster's name, for now** (§5.2), as
   recommended. A known difference from Word until `152` §10 Q2's subtree walk exists.
6. **The grant encoding is JWT** (owner decision 2026-10-23; closes `143` §16 Q5). The grant's
   subject and profile are JWT claims, signed by the host; the relay verifies the signature
   and refuses an identity that differs from the grant's subject. Algorithm choice and key
   distribution follow standard JWT practice (asymmetric signatures, so the relay holds only
   the public key).

---

## 13. Build plan (HF-360): one PR, in this order

1. **Engine and protocol.** `Verified`, the subject-equals-identity rule, `Profile`,
   `Participant`, `Join.profile`, `Arrival.author`, the two new variants and
   `PROTOCOL_VERSION` 7. Journal record compatibility for `Arrival.author`. Guards: a
   grant for A with identity B is refused (driven red by removing the check); a source scan
   that `Submission` has no author field (the `152` §2b method); open-room avatars dropped.
2. **`authorship::conform`** in `casual-doc-edit`, at the `localise` choke point. Guards: two
   replicas conform a forged arrival identically; an existing comment's author cannot be
   rewritten; an honest commit is a no-op on arrival (no rollback). Each is driven red.
3. **wasm.** `collabJoinFrame(…, grant)`, roster adoption, and `setActiveAuthor` fed from
   `me`.
4. **Webapp.** The identity axis in `capabilities.mjs`, `embed_element.mjs`,
   `host_contract.mjs` and `host_options.mjs` (the `149` rows `user.id`, `user.image` and
   `mentionShare` move from Gap). `participantDisplay` (§14). Settings disabled with a reason.
   The "You" rendering rule. The directory picker. The
   people list.
5. **Versions.** The three provider methods, the two groups in the panel, and the reference
   host (`examples/share-host.mjs`) implementing all of it, with HMAC-signed grants clearly
   marked as a development key.
6. **e2e.** Two clients with different granted identities: names on carets, names on each
   other's comments, a forged-author client corrected on the other screen, `@` with and
   without a directory, and a shared version authored by B visible to A.

---

## 14. The `participantDisplay` seam

A build lane is adding the presence avatar strip now, with initials derived from today's
numbered labels behind one function. This is that function's contract, fixed here so the
strip upgrades with **no UI change** when HF-360 lands.

```ts
// Input: one roster entry as the session reports it.
interface RosterParticipant {
  client: number;               // relay participant number
  profile?: {                   // absent today; present after HF-360
    id: string; name: string; initials?: string;
    color?: string;             // "#rrggbb", attested only (§4.2)
    avatarUrl?: string;         // already filtered by the relay (§4.2) and bounds (§4.4)
  };
  attested: boolean;            // false today
  isSelf: boolean;
}

// Output: everything the strip, the caret flag and the people list paint.
interface ParticipantDisplay {
  label: string;      // profile.name, else t("collab.participant", { number: client + 1 })
  initials: string;   // profile.initials, else derived from name (first graphemes of up to
                      //   two words, as setActiveAuthor derives them), else String(client + 1)
  color: string;      // profile.color if attested and valid, else today's palette for client
  avatarUrl: string | null;  // non-null ONLY if profile.avatarUrl is set AND the mount's
                             //   avatars policy is "fetch" (§8.1); otherwise null, and the
                             //   chip paints initials
  verified: boolean;  // = attested; false shows the "unverified" mark when a name is present
  isSelf: boolean;
}
```

Rules the strip may rely on:

- `participantDisplay` is **pure and synchronous**, and the only place these rules live.
  The avatar policy is read from the mount configuration it is constructed with, not from a
  global.
- **The strip never builds a URL, never fetches anything itself, and never reads `profile`
  directly.** When `avatarUrl` is non-null it renders
  `<img referrerpolicy="no-referrer" decoding="async" loading="lazy" alt="">`, with the
  label carried by the chip's accessible name. On `error` it falls back to initials.
- Today every entry has no `profile` and `attested: false`, so the function returns exactly
  today's label and palette. The strip built now is the strip that ships.

---

## 15. As built (HF-360), and where it differs from the text above

| Section | As built | Why |
| --- | --- | --- |
| §3.3, §12 Q6 | The grant is a compact JWS: `{"alg":"EdDSA"}` over JWT claims `sub`, `aud`, `exp`, `nbf`?, `role`, and the display claims `name`, `initials`, `color`, `picture` (OpenID Connect's name for an avatar). `opendoc_relay::JwtVerifier` accepts **only** EdDSA over Ed25519 (RFC 8037) and holds only the public key; `opendoc-relay serve <journal> <addr> granted <key> <audience>`. | One algorithm, so neither `alg: none` nor key confusion is reachable. `role` maps onto the existing capability presets, so a grant cannot spell a combination the command line cannot. |
| §4.1 | `GrantVerifier::verify` returns `Verified { capabilities, subject, profile }`, and the subject rule lives in `Access::admit`, so it holds for every verifier, not only the built-in one. A grant whose profile `id` is not its subject is refused the same way. | A rule each verifier had to remember is the rule `access.rs` used to call optional. |
| §4.2 | An open room's claimed profile takes the join's identity as its `id`, whatever the client wrote. A claimed profile out of bounds stops the join with `ODC-7007`. | A participant must not claim a second person's id either. |
| §4.5 | The author stamp is journalled inside each `Admission` and on each ordered entry, without the avatar; the relay's in-memory list carries the attested avatar. | Recovery replays admissions before chunks, so a crash cannot change whose name a later chunk carries. |
| §5.2 | `conform` decides "introduced" by the **id space** of the identity that carries the mark (the node, or for a run-format change its editor group): held → keeps its author; not held and minted by the sender → the stamp; not held and minted elsewhere → kept (an undo of "accept" puts back somebody else's suggestion). An introduced comment's `person` is cleared unless it already names the stamp, rather than set to the name, because the editor's own lowering writes none and an honest arrival must stay a no-op. | Without the third rule an undo would hand a restored suggestion to whoever pressed undo. **Known limit:** a rewritten client that mints a new mark in *another* participant's space reaches that rule and keeps its forged author; telling it from a genuine undo needs a record of who minted what, which no replica keeps. Table row and cell marks are not conformed (no editor command authors one). |
| §5.2 | Conform runs in `apply_remote`, at the horizon, on both the uncontended and the contended path, and in the editor's `apply_group` with the room's `me`. In a room the facade also authors under `me` rather than the Settings name. | The horizon is the ordered state every replica shares; a comment this replica deleted in unordered work must still read as held. |
| §6.2 | `setAuthorPersistence(providerId, userId)` opts the active author in; at save time a `w15:person` with `w15:presenceInfo` is added for the active author if they authored a comment or a change and `people.xml` has no entry for the name. An existing entry under another account refuses the save with `identity-name-collision`. | §12 Q3 settled the display half (a "(2)" suffix that never reaches the file); the file half stays a refusal, because merging two accounts under one name is the silent loss §1 forbids. |
