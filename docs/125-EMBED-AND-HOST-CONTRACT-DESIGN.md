# 125 — Embed and Host Contract Design (D-6)

**Status:** Proposal, for the owner decision recorded as `109` Q1 / D-6.
**Supersedes nothing.** Extends `83` (SDK packaging), `104` §HF-109 and §D-5/D-6,
`106` Phase 4. Blocks: HF-109, CQ-010, RM-02, and Phase 4 — which `106` calls
"the wedge".

Every figure below was re-derived on the date of writing. Where an existing doc
disagrees, this document says so rather than repeating it.

---

## 1. Why this is the wedge, stated precisely

ONLYOFFICE is AGPL-3.0-only, Developer Edition is billed **per concurrent
browser tab**, and the host-customization API is gated *in code* —
`LayoutManager._applyCustomization` early-returns when `!_licensed`. A permissive
licence plus real DOCX fidelity plus embeddability is an unoccupied position.

So the embed contract is not a late-phase nicety. It is the product thesis, and
`105` CQ-010 states the consequence plainly: *"For a project whose stated
position is an embeddable runtime, this is the gap that makes the position
unclaimable."*

**Corollary that governs every decision below:** anything ONLYOFFICE puts behind
its licence gate, we ship ungated. That is the entire point, and it is free for
us to do.

### 1.1 Exactly what their licence gates — verified, and it is two gates

Read from source at `web-apps@9c0ca538` / `sdkjs@72b0421`, not from their docs.

There are **two independent, server-supplied flags**, both defaulting to `false`
(`sdkjs/common/apiCommon.js:473-474`), both arriving over the co-authoring
socket (`apiBase.js:1981-1986`):

```js
// apps/common/main/lib/controller/LayoutManager.js:66-68
var _applyCustomization = function(config, el, prefix) {
    !config && (config = _config);
    if (!_licensed || !config) return;          //  ← gate 1: canBrandingExt
```
```js
// apps/documenteditor/main/app/controller/Main.js:1803, 1809
canBrandingExt = params.asc_getCanBranding() && …   //  chrome layout, plugins
canBranding    = params.asc_getCustomization();     //  ← gate 2: logo, About
```

| Unlicensed you still get | Unlicensed you lose |
|---|---|
| theme/colours, `unit`, `zoom`, `autosave`, tab style, `feedback`, `goback`, anonymous naming, macro policy, `compactToolbar`, `hideRightMenu`, review display defaults | **all** `customization.layout.*` chrome hiding (22 named regions), header **logo**, the About **licensee block**, UI plugins, `features.spellcheck.change`, `reviewPermissions`, `submitForm` |

Three details worth stating because they are the sharp end:

1. **You cannot remove their About button.** `Main.js:2639-2640` force-sets
   `customization.about = true` when `!canBrandingExt`. Setting it to `false` is
   silently reverted.
2. **A custom loader logo or app font is not blocked — it is *nagged*.** It
   applies, then `Main.js:1650-1662` raises a *"paid feature — contact us"*
   modal. Worse than refusing, because the integrator ships it and then
   discovers the modal.
3. **Removing one gate would not white-label it.** The logo and About sit behind
   `canBranding`, the chrome behind `canBrandingExt`. Anyone reasoning about
   "just patch the licence check" has to find both.

Editing itself is also revoked at runtime on connection limits — `disableEditing(true)`
plus `asc_coAuthoringDisconnect()` with a "Buy now" modal (`Main.js:1591-1663`).
That is the per-concurrent-tab meter, enforced in the client.

**Our position, concretely:** every row in the right-hand column above is a
plain config field here, with no flag, no meter, no modal, and no About button
we refuse to let a host remove.

---

## 2. What we actually have — audited, not assumed

The honest starting position, verified in source.

### 2.1 Already built and good

| Asset | Evidence |
|---|---|
| Three enforced modes | `reviewMode` = `editing\|suggesting\|viewing`, `main.js` |
| **Two fail-closed choke points** | `blockMutationInViewing()` — 26 call sites; `blockUntrackedInSuggesting()` — 14, both through `runEdit` |
| Engine-authoritative document lock | `editingUnavailableReason` (wasm getter) → `readOnlyReason`, forces `viewing`, supplies human text |
| One shared refusal-message policy | `edit_errors.mjs`, unit-tested, consulted by both pre-engine and engine paths |
| ~92-command registry with reasons | `editorCommands()` → `{ id, enabled, disabledReason, run, … }` |
| Per-object capability bits, fail-closed | `canAltText/canCrop/canDelete/canEditText/canFill/canMove/canResize/canStroke/canWrap` |
| Engine multi-document | `WasmDocument` is per-document; `open()` is a free function |
| 19 locales with `?lang=` precedence | `webapp/locales/*.json` (19 files) |
| Three-state theming, tokens centralised | 112 token names, 1,165 `var()` uses, every hex inside the token blocks |
| Import/export loss detection | `reportJson`, `importReportJson`, `missingCoverage()` |

`106` §448 and `105` UX-009 claim one locale. **Stale — there are 19.** SKILL §11
says 140 tokens in 6,481 CSS lines; the file is now 8,072 lines with **112**
tokens. Re-derive before publishing either number.

### 2.2 The four findings that shape the design

**F1 — The typed host facade already exists, and the product does not use it.**
`crates/casual-doc-sdk` ships `Engine`, `EngineConfig`, `DocumentSession`,
`SessionId`, `Subscription`, `RuntimeEvent`, `SequencedEvent`, `EventBatch`
(with `dropped_events` back-pressure), `SdkError`/`ErrorCode`/`ErrorSeverity`,
`DocumentSnapshot`, `PositionMap`, `TransactionResult` — exactly the vocabulary
`83` promises. **Verified: the only crate that depends on it is
`tools/opendoc-benchmark`. `casual-doc-wasm` does not.**

Its command set is 5 typed requests (`Insert`, `Delete`, `Split`, `Join`,
`SetSelection`); `casual-doc-wasm` applies `casual-doc-edit`'s 47 ops directly
across 340 flat exported names. This is the *same* op-set split SKILL §2 records
as the collaboration blocker, one layer up. **One substrate fixes both.**

Writing a host contract on top of the 340-name facade would freeze the wrong
boundary.

**F2 — `main.js` cannot be imported, and the seam must be extracted, not added.**
Zero exports. 357 module-scope element binds, 180 top-level `addEventListener`
on them, 128 top-level `let` singletons, page-global `document.body` classes. A
missing id is a `TypeError` at module evaluation that kills the rest of the file.
And `main.js` is **17,054 lines against a 17,054 ceiling** — zero slack. The
mount seam is an extraction exercise by construction.

**F3 — A framed editor today is a standalone editor.** There is no host
capability concept, so File ▸ New and File ▸ Open are live inside someone else's
page. opencalc measured and fixed this exact defect (`104` §HF-109): a framed
editor resolved to `standalone`, and a visitor could replace the host's document
from inside the host's own page. Two things here already know about framing —
autosave defaults off when `window.self !== window.top`, and one command carries
the reason *"Autosave is off in an embedded editor"* — which is proof the
direction is right and was simply never generalised.

**F4 — Two things silently defeat white-labelling.**
`main.js` runs `applySettings()` at import, which writes an **inline**
`--accent` and `data-theme` on `:root`; an inline style beats any host
stylesheet, so a host's brand colour is overwritten at boot from `localStorage`.
And `documentTabTitle()` returns `` `${name} — OpenDoc` `` — **the host page's
own browser tab title carries our brand**, guarded by a test. 21 hardcoded brand
occurrences across 9 files; none are in the locale catalogues, so i18n does not
reach them.

---

## 3. Architecture

Four layers, one owner each. Layers 2 and 4 largely exist; 1 does not; 3 is
partly there.

```
                    host application
                          │
        config ──────────►│◄────────── events
        methods ─────────►│
┌─────────────────────────┴──────────────────────────────┐
│ 1  EMBED FACADE          createEditor(el, config)      │  NEW
│    instance lifecycle · config validation · teardown   │
│    <opendoc-editor> custom element · iframe bridge     │
├────────────────────────────────────────────────────────┤
│ 2  POLICY                capabilities · permissions    │  EXTEND
│    role → capability set → per-operation permit()      │
│    surfaces reasons through edit_errors.mjs            │
├────────────────────────────────────────────────────────┤
│ 3  UI SHELL              chrome regions · branding     │  EXTEND
│    declarative region walker · design tokens · i18n    │
├────────────────────────────────────────────────────────┤
│ 4  RUNTIME               casual-doc-sdk → wasm         │  CONVERGE
│    typed commands · RuntimeEvent · SdkError            │
└────────────────────────────────────────────────────────┘
```

### 3.1 Two embed shapes, one contract (owner decision: both)

- **Direct module** — `import { createEditor } from "@casualoffice/document-runtime"`.
  This is the differentiator: ONLYOFFICE **cannot** do it, because their format
  I/O is `x2t` and `core/X2tConverter/build/` has only `Android/` and `Qt/` —
  no WASM build. We are a library; they are a hosted app.
- **iframe + postMessage** — the same contract, mirrored over a typed envelope,
  for hosts that want isolation or are porting from `DocsAPI`.

The iframe bridge is a **thin adapter over the module API, not a second API.**
One contract, two transports. A second hand-written protocol is how the two
diverge.

### 3.2 Enforcement is layered (owner decision: both)

`104` D-5 already decided this and it is the most load-bearing prior decision:
permissions are enforced **per operation, deny-by-default — not by hiding
chrome**. Hiding is UX; refusing is policy.

```
UI layer      command.enabled = false, disabledReason = "…"   ← honest UI
JS choke      runEdit() → permit(op) before blockMutation*    ← one gate
Engine        apply_group / export refuses, returns reason    ← real boundary
```

A host that says `download: false` must not be defeated by devtools. That means
the engine is the authority and the UI is a *reflection* of it — which is exactly
how `readOnlyReason` already works today, so this generalises an existing shape
rather than inventing one.

---

## 4. Configuration (LLD)

One object, versioned. ONLYOFFICE's config is flat and unversioned; ours declares
its version so the contract can evolve without guessing.

```ts
createEditor(target: Element, config: EditorConfig): EditorInstance

interface EditorConfig {
  version: 1;                        // required; refuse unknown majors

  document?: {
    source?: ArrayBuffer | Blob | URL | null;   // null ⇒ blank
    name?: string;                              // shown in chrome + tab title
    format?: FormatId;                          // else sniffed
    password?: string;                          // §6
  };

  access?: Role | AccessSpec;        // §5 — default derived from framing
  user?: { id: string; name: string; initials?: string };

  ui?: {
    chrome?: "ribbon" | "compact" | "none";
    regions?: Partial<Record<RegionId, boolean>>;   // declarative, §7
    theme?: "system" | "light" | "dark";
    tokens?: Record<string, string>;               // design-token overrides
    locale?: LocaleTag;                            // one of the 19
    branding?: {
      name?: string | null;          // null ⇒ no product name anywhere
      logo?: string | null;          // URL or null
      tabTitle?: "document" | "document+product" | "host";
    };
  };

  behaviour?: {
    autosave?: boolean;              // default false when framed (existing)
    fonts?: FontSource[];            // host-provided faces
    zoom?: number | "fit-width" | "fit-page";
  };

  on?: Partial<EditorEvents>;        // §8, or addEventListener
}
```

**Rules.**
1. `version` is required and unknown majors are **refused, not coerced** —
   failing closed is the house rule.
2. Config is validated **before first paint**. `104` is explicit that branding
   and capability gating must apply before the first frame, not be swapped after.
3. Unknown keys are an error in development and ignored in production, reported
   through `onWarning`.

---

## 5. Roles, permissions and modes

Three distinct concepts that today are conflated. Keeping them separate is what
makes the model honest.

| Concept | Who owns it | Example |
|---|---|---|
| **Role** — what this user is allowed to be | the **host** | `commenter` |
| **Mode** — what the user is currently doing | the **reader** | `suggesting` |
| **Document lock** — what the file itself permits | the **engine** | `documentProtection w:edit="forms"` |

The effective capability is the **intersection of all three**, and the most
restrictive wins. `readOnlyReason` already behaves this way; roles join it.

### 5.1 Roles (owner request)

Presets over one capability set, not a parallel system:

| Role | Read | Comment | Suggest | Edit | Export/print | Change permissions |
|---|---|---|---|---|---|---|
| `owner` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `editor` | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| `commenter` | ✅ | ✅ | — | — | configurable | — |
| `reviewer` | ✅ | ✅ | ✅ | — | configurable | — |
| `readonly` | ✅ | — | — | — | configurable | — |
| `preview` | ✅ | — | — | — | — | — |

`preview` is deliberately the floor: `83` already specifies a `{ readOnly: true }`
preview that **bypasses the mutation, caret and IME paths entirely** — cheaper and
safer than a disabled editor, because the code that could mutate is not loaded.

`readonly` vs `preview` is a real distinction: readonly is an editor you cannot
type in (selection, find, navigation, export); preview is a viewer.

**Default when framed is `readonly`, not `editor`** — generalising the autosave
precedent and closing F3. A host that wants editing asks for it.

### 5.2 Per-capability override

Roles are presets over an explicit set, so a host can be precise:

```ts
type AccessSpec = {
  role: Role;
  can?: Partial<Record<Capability, boolean>>;   // narrows only, never widens
};

type Capability =
  | "edit" | "comment" | "suggest" | "acceptReject"
  | "download" | "print" | "copy"
  | "insertMedia" | "editHeaderFooter" | "manageBookmarks"
  | "changePermissions";
```

`can` may only **narrow** the role. A host cannot grant `edit` to `preview` by
accident, and there is exactly one direction to audit.

### 5.3 Command filtering falls out for free

The command registry already carries `{ enabled, disabledReason }`. Policy adds
one input; no `if (config…)` enters feature code. A denied command shows
**disabled with the host's reason** — never missing, never dead. That is the
existing UI floor, unchanged.

---

## 6. Password handling — honest scoping

**This is engine work, not configuration.** Verified: zero hits for `encrypt`,
`password` or `Signature` in `casual-doc-wasm` *or* the model crates. Document
encryption does not exist at any layer today.

Three separate features that are routinely conflated:

| Feature | What it is | Size |
|---|---|---|
| **Open password** | ECMA-376 Agile Encryption; file is ciphertext | **L** — real crypto, in the engine |
| **Modify password** | Opens read-only unless the password is given | **M** — depends on the above |
| **Permissions password** | `w:documentProtection` with a hash | **S–M** — the enforcement half already exists |

The third is the nearest: the engine **already understands**
`w:documentProtection w:edit="forms"` and refuses edits with an explained reason
(`refused: …`). So enforcement exists; what is missing is the credential.

**Recommendation.** Do not put passwords in v1 of the embed contract. Reserve
`document.password` in the schema so the shape is stable, and implement:
1. `documentProtection` credential handling (reuses existing refusal path),
2. then open/modify password as its own ADR, because AES/Agile Encryption is a
   security surface that deserves its own review and its own threat model.

Shipping a password field that only hides UI would be **security theatre** and
directly violates §3.2.

---

## 7. White-label (owner decision: branding + chrome + theme)

**Branding.** `ui.branding` controls name, logo and tab title. Requires removing
21 hardcoded occurrences across 9 files — notably `status_policy.mjs`, whose
`documentTabTitle()` brands the **host's** tab, and `editor.html`'s favicon,
canonical URL and two logo `<img>` tags. None are in locale catalogues.

**Chrome regions.** One declarative walker over declared region ids, as `106`
§294 prescribes: *"~120 knobs, zero `if (config…)` in feature code."* The
predictable failure mode is config checks scattered through 92 command
descriptors and ~50 ad-hoc `.disabled =` sites — SKILL §10's "fix the class, not
the instance", stated in advance.

**Theme.** Tokens are already in good shape. Two fixes required:
1. `applySettings()` must **not** write an inline `--accent`/`data-theme` when
   the host supplied them (F4). Host config outranks `localStorage`.
2. Shadow-DOM encapsulation with `all: initial` on `:host`, as opencalc proved,
   so the two cascades stop colliding in both directions.

**Ungated, always.** No licence check, no tab counting, no `_licensed` branch.

---

## 8. Events and methods

Typed, versioned, and **derived from `casual_doc_sdk::RuntimeEvent` rather than
invented** — which is how F1 gets repaid instead of worked around.

`RuntimeEvent` has exactly two variants today (`TransactionCommitted`,
`SelectionChanged`). The host-facing set extends it:

```ts
interface EditorEvents {
  onReady(e: { version: string; locale: LocaleTag }): void;
  onDocumentOpened(e: { name: string; format: FormatId; pages: number;
                        findings: CompatibilityFinding[] }): void;   // ← §8.1
  onDirtyChanged(e: { dirty: boolean }): void;
  onStateChanged(e: { state: "opened" | "edited" | "saved" }): void;
  onSelectionChanged(e: { hasRange: boolean; inTable: boolean }): void;
  onModeChanged(e: { mode: Mode; reason?: string }): void;
  onCommand(e: { id: string; allowed: boolean; reason?: string }): void;
  onExported(e: { format: FormatId; bytes: ArrayBuffer;
                  findings: CompatibilityFinding[] }): void;
  onWarning(e: { code: WarningCode; message: string }): void;
  onError(e: { code: ErrorCode; severity: ErrorSeverity; message: string }): void;
}
```

`ErrorCode`/`ErrorSeverity` are `casual_doc_sdk`'s, not new ones.

### 8.1 The differentiator nobody else can ship

`onDocumentOpened.findings` surfaces **import** loss. Today `importReportJson`
is a shipped wasm getter with **zero webapp consumers** — import-time loss is
computed and thrown away, while export-time loss is surfaced.

SKILL §1 names this as the condition under which our verbatim-retention
advantage is real at all: *"this advantage is only real if loss is detected **and
reported**."* It is structurally impossible for ONLYOFFICE, whose pipeline is
DOCX → Editor.bin → DOCX.

**Smallest genuine win on the whole list** and it should land first.

### 8.2 Methods

```ts
interface EditorInstance {
  open(source, opts?): Promise<void>;
  export(format: FormatId): Promise<{ bytes: ArrayBuffer; findings: [] }>;
  setAccess(access: Role | AccessSpec): void;      // narrowing takes effect live
  setMode(mode: Mode): void;                        // refused if role forbids
  setTheme(theme, tokens?): void;
  setLocale(locale: LocaleTag): void;
  run(commandId: string, arg?): Promise<CommandResult>;   // over the registry
  getState(): EditorState;
  destroy(): void;                                  // real teardown, ~100 MB
}
```

`run()` over the existing ~92-command registry means the host gets every command
the product has, and gets new ones for free — rather than a hand-maintained
parallel list that drifts.

---

## 9. Phasing

Ordered by what unblocks what. Sizes are this repo's grading.

| # | Work | Size | Unblocks |
|---|---|---|---|
| 0 | **Answer D-6** (this document) | S | everything |
| 1 | Surface **import findings** through the existing export path | **S** | §8.1 — ship first, standalone value |
| 2 | **Capability presets** (`standalone`/`embedded`/`viewer`), framed ⇒ embedded, resolved before first paint | **M** | closes F3 today, no SDK needed |
| 3 | Branding de-hardcode + host-outranks-localStorage theming | **S** | white-label |
| 4 | Extract `mount(el, config)` out of `main.js` | **L** | HF-109; gated on HF-085 extraction |
| 5 | Converge wasm onto `casual-doc-sdk` commands/events | **L** | F1; also unblocks OT (SKILL §2) |
| 6 | Per-operation `permit()` at `runEdit` + engine | **L** | real permissions |
| 7 | `<opendoc-editor>` + shadow DOM + iframe bridge | **M** | isolation, `DocsAPI` porting |
| 8 | Package: `exports`, hand-written `.d.ts`, consumer type-check job | **M** | RM-02 |
| 9 | Password: `documentProtection` credential, then encryption ADR | M→L | §6 |

**Items 1–3 are independently valuable and need no SDK.** They close the
embarrassing half of F3 and F4 within days. Items 4–6 are the real work.

The gate `106` sets for Phase 4 stands: *a third-party integrator embeds
OpenDoc in a closed-source app with no server, drives it through the documented
config/permissions/events contract, localises it, and passes an accessibility
audit — using only published packages and public docs.*

---

## 10. What this asks the owner to approve

1. **D-6 answered** as: extend `83`, borrow opencalc's two proven mechanisms
   (capability presets with framed⇒embedded default; hand-written `.d.ts` plus a
   consumer type-check job so the declaration cannot drift).
2. **Layered enforcement** (UI + engine), per-operation, deny-by-default.
3. **Both transports**, one contract — module first, iframe as an adapter.
4. **Roles** as presets over one capability set; `can` narrows only.
5. **Passwords out of v1**, schema slot reserved, encryption gets its own ADR.
6. **`casual-doc-sdk` convergence** accepted as the substrate rather than
   freezing the 340-name wasm facade.

## 11. Borrowed and rejected, from the ONLYOFFICE source audit

Their integration API is ten years of production experience and it would be
foolish to ignore it. It is also full of decisions we should not repeat.

### 11.1 Borrow

- **Capability-by-handler-presence.** `api.js:407-434` derives 27 `can*` booleans
  from `!!events.onX`, so a host cannot enable an affordance it has not
  implemented. That is our own "never a dead control" rule, arrived at
  independently. Borrow the mechanism; make it typed and derived, not 27
  hand-written lines.
- **Request/response pairing with a correlation tag.** `onRequestUsers` → `setUsers`
  carries `c` and the reply is rejected when it does not match. Borrow it, but
  use a real request id rather than a semantic string.
- **Two-phase handshake.** `onAppReady` → host sends `init` + `openDocument`
  separately, so configuration and document are independent and a warm frame can
  open a second document.
- **Zero-copy binary.** `openDocumentFromBinary` / `onSaveDocument` transfer an
  `ArrayBuffer`. For a local-first engine this is the *primary* path, not an
  optimisation.
- **`warmUp()`.** Pre-loading the bundle into a hidden frame is a cheap, real
  improvement to perceived open time.
- **Deprecation logged at the read site**, with the old key still working.

### 11.2 Reject, with the reason

- **`callbackUrl` + server-held key.** Never used by their client; handed to the
  document server in the socket `auth` frame together with `permissions` and
  `mode`. So the *browser is the channel through which authorization reaches the
  server*, and there is no client-side token verification at all
  (`apiCommon.js:6483` is a bare setter). Our host is the authority; nothing
  round-trips authorization through the editor.
- **Permissions that are silently UI-only.** Only `edit`, `comment`, `fillForms`
  and `copy` reach their engine. `print`, `download`, `protect`,
  `modifyContentControl`, `review`, `chat`, `editCommentAuthorOnly`,
  `deleteCommentAuthorOnly` are enforced by hiding buttons — and `permissions.reader`
  and `modifyFilter` are read *nowhere* in the document editor. An integrator
  cannot tell which half they are getting. **This is the direct justification
  for §3.2:** if a permission cannot be enforced in the engine, the schema must
  say so rather than implying a guarantee.
- **No config versioning.** The decay is visible in their own surface:
  `toolbar`/`leftMenu`/`rightMenu`/`statusBar` each superseded by `layout.*`,
  `onRequestCompareFile` → `onRequestSelectDocument`, `onOutdatedVersion` →
  `onRequestRefreshFile`. Hence `version: 1` being **required** in §4.
- **Four defaults for one key.** `fillForms` defaults differently in `api.js`,
  DE-main, the forms app, the PDF editor and mobile. Defaults belong in one
  normalization function, once.
- **Group permissions by string-splitting a display name.** `user.group` is
  concatenated into `fullname` separated by `String.fromCharCode(160)` and parsed
  back out by splitting on the NBSP. Group-scoped review and comment permissions
  depend on that. Ours will carry structured identity or none.
- **`postMessage(..., "*")` in both directions**, both marked
  `// TODO: specify explicit origin`, shipped — while `init` carries
  `callbackUrl` and `openDocument` carries the document URL and token. We pin the
  target origin, and it is not optional.
- **Validation by `window.alert`.** An invalid config alerts and returns an
  object with no working methods. Ours fails closed with a typed error.
- **Two configuration channels.** Chrome options travel in the **iframe URL**
  (`customer`, `logo`, `uitheme`, `compact`) and are interpolated into markup by
  `document.write` behind a four-character escaper, while everything else goes by
  `postMessage`. One channel, validated once.
- **Permissions derived exactly once, with no true revocation.**
  `denyEditingRights` disconnects the socket and greys the UI without mutating a
  single flag; the in-memory document stays editable. Our `setAccess()` must
  actually narrow, live.

## 12. Open questions

- **Q-A** Does `owner` mean anything without a persistence/identity story, or is
  it a host-asserted label we simply reflect? (Storage is decided YES, opencalc
  shape — but identity is not.)
- **Q-B** Do we publish the design-token names as a **public contract**? Doing so
  freezes them; not doing so makes theming unsupportable.
- **Q-C** Does the iframe bridge need `DocsAPI` *shape* compatibility (so their
  integrators port by renaming) or only equivalent capability?
