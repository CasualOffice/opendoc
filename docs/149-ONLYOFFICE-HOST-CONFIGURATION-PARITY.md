# 149 — ONLYOFFICE host configuration, option by option

**Status:** live. The tables below are GENERATED from `webapp/src/host_options.mjs`
by `webapp/tools/build-embed-docs.mjs`, and `--check` runs in `webapp/build.sh`, so
they cannot drift from the code that implements them. Edit the module, re-run the
tool, commit both.

## Why this document exists

The owner's instruction was *"i need customisation embeddability like ONLYOFFICE"*.

ONLYOFFICE's host surface is one object — `permissions`, `editorConfig`, and
`editorConfig.customization` — documented in their own source at
`apps/api/documents/api.js:94-320`. It is also the thing their licence gates. Every
`customization` key funnels through `Common.UI.LayoutManager._applyCustomization`,
and its second line is:

```js
var _applyCustomization = function(config, el, prefix) {
    !config && (config = _config);
    if (!_licensed || !config) return;
```

`_isElementVisible` and `_getInitValue` beside it carry the same test. So an
integrator who configured the editor and did not buy a licence gets an editor that
ignores them — **silently**, because an early return has nothing to say. Their
Developer Edition is billed per concurrent browser tab.

Ours is Apache-2.0 and **ungated**. That is the wedge (`SKILL` §1), and matching
their *surface* while charging nothing for it is the product. This document is the
audit that says how far the surface actually matches, and it is generated rather
than written so it cannot quietly become marketing.

## The three axes a host configures

Their `customization` block mixes three different kinds of thing under one name.
Ours keeps them apart, because collapsing them would be wrong in ways that matter,
and all three resolve in one authority — `webapp/src/capabilities.mjs`.

| Axis | What it is | How a host says it | Rule |
| --- | --- | --- | --- |
| **Capabilities** | permissions: may this container ever do this? | `mode`, `can=-print` | A host list may only **narrow** a role. There is exactly one direction to audit. |
| **Regions** | composition: which chrome this container paints | `chrome=-ruler` | "Never, for you" is silent. A withheld region is a presentation decision, never a permission. |
| **Preferences** | opening positions the visitor may then change | `prefs={"user":"Ada"}` | Neither narrowed nor widened — a default, and a visitor's own stored choice still wins. |

The third axis is new with this work and is where most of their `customization`
block lands: their own comments say *"init value in de/pe"*, *"init value for right
panel"*. It could not be folded into either of the others. A preference held to the
narrow-only rule is unexpressible — `spellcheck: false` takes nothing away — and one
treated as a region would say "never, for you" when it means "start here".

**Composition is decided once, at boot.** There is no channel that lets a live
container widen itself, and that refusal is deliberate: the resolution is written
into the frame's first URL, so there is no window in which the editor is one thing
and is then told it is another.

## What a host actually writes

Three spellings, in increasing order of how much the host already has:

```html
<!-- ours, declarative -->
<opendoc-editor mode="commentor" can="-download" chrome="-ruler,-review"
                prefs='{"user":"Ada Lovelace","spellcheck":false}'
                frame-title="Contract draft"></opendoc-editor>

<!-- theirs, handed over verbatim -->
<opendoc-editor config='{"permissions":{"print":false},
                         "editorConfig":{"lang":"fr","user":{"name":"Ada Lovelace"},
                         "customization":{"hideRulers":true,"macros":true}}}'></opendoc-editor>
```

The second is the migration surface. It is lowered onto `mode`, `can`, `chrome`,
`prefs` and `lang`, and **everything that cannot be honoured is reported** — on the
`opendoc-capabilities` event as `detail.notes`, and once to the console. Nothing is
silently dropped. That is the same rule this repository already holds import loss
to, applied to configuration, and it is the half `_applyCustomization` cannot offer
at all.

The narrowing property survives the translation, which was the part that mattered.
Their booleans go both ways and ours go one, so only `false` produces anything; a
`true` the resolved role does not grant comes back as `cannot-widen`, naming `mode`
as the place to ask, rather than being honoured or dropped.

## The tally

<!-- @generated option-tally -->
| Verdict | Options | What it means |
| --- | --- | --- |
| Have it | 42 | lowered onto one of our three axes, and the resolved container really changes |
| Have it, differently | 8 | honoured in part; the note says what is different and why |
| Refused by design | 0 | it would GRANT something, and a host list may only narrow — reported, naming `mode` |
| Gap — work named | 41 | the editor has no such state or surface yet; the note names the work |
| Declined | 21 | deliberately not wanted, with the argument |
| Another product | 9 | belongs to spreadsheets, presentations or PDF forms (`SKILL` §1) |
| **Total enumerated** | **121** | every leaf of their `permissions`, `editorConfig` and `customization` |
<!-- @end option-tally -->

## Option by option

`Ours` is what a host says to us instead. A blank means there is nothing to say —
the note gives the reason.

<!-- @generated option-map -->
| Theirs | Ours | Verdict | Note |
| --- | --- | --- | --- |
| `permissions.edit` | `mode / can=-edit` | Have it | A container that may not edit is the `readonly` role, or `can=-edit` over any role. |
| `permissions.download` | `can=-download` | Have it | — |
| `permissions.print` | `can=-print` | Have it | — |
| `permissions.comment` | `can=-comment` | Have it | Withholding it also closes Suggesting mode: `editingModeFor` reads the same grant. |
| `permissions.reader` | `mode=readonly` | Have it | A role, not a flag. Composition is decided at boot, so it travels in `mode`. |
| `permissions.review` | `mode=commentor` | Have it, differently | `commentor` is the role: annotate and suggest, never commit. There is no separate switch for 'may edit but may not review', because a tracked change is an edit. |
| `permissions.copy` | — | Gap — work named | No clipboard gate exists. `edit.copy` is deliberately ungated today — nothing in the capability set covers reading, and a `preview` is a usable reader. Adding one is a capability plus a choke point in the copy path (webapp/src/clipboard.mjs), not chrome. |
| `permissions.protect` | — | Gap — work named | Document protection is not implemented; there is no Protect surface to show or hide. |
| `permissions.fillForms` | — | Another product | PDF forms are out of scope (SKILL §1). |
| `permissions.modifyContentControl` | — | Gap — work named | Structured document tags are modelled and round-tripped, not separately gated. |
| `permissions.modifyFilter` | — | Another product | Spreadsheets are opencalc's (SKILL §1). |
| `permissions.chat` | — | Declined | There is no chat and there is not going to be one in the editor: a chat panel is a messaging product wearing an editor's chrome, and the host already has one. Collaboration here is documents — presence, comments, suggestions (ADR-033). |
| `permissions.editCommentAuthorOnly` | — | Gap — work named | Needs per-comment authorship enforcement, which lands with collaboration identity. |
| `permissions.deleteCommentAuthorOnly` | — | Gap — work named | Same identity work as `editCommentAuthorOnly`. |
| `permissions.reviewGroups` | — | Gap — work named | Per-GROUP review rights need multi-user identity, which arrives with collaboration (ADR-033, docs/107). `mode=commentor` is the single-user shape of the same intent. |
| `permissions.commentGroups` | — | Gap — work named | Same identity work as `reviewGroups`. |
| `permissions.userInfoGroups` | — | Gap — work named | Presence is not shipped, so there is no user list to filter. |
| `editorConfig.mode` | `mode` | Have it | Their two values become one of 8 names; "view" is `readonly`. |
| `editorConfig.lang` | `lang` | Have it | — |
| `editorConfig.user.name` | `prefs={"user":…}` | Have it | Signs this visitor's comments and tracked changes. |
| `editorConfig.user.id` | — | Gap — work named | A stable identity matters when there are several; it lands with collaboration. |
| `editorConfig.user.group` | — | Gap — work named | Only meaningful with `reviewGroups`/`commentGroups`, which need identity. |
| `editorConfig.user.image` | — | Gap — work named | No avatar surface: presence is not shipped. |
| `editorConfig.region` | — | Gap — work named | Number and date formatting follow the document; there is no per-host override yet. |
| `editorConfig.coEditing` | — | Gap — work named | Co-editing is designed (ADR-033, OT over transactions) and not shipped. |
| `editorConfig.canCoAuthoring` | — | Gap — work named | Same as `coEditing`. |
| `editorConfig.callbackUrl` | — | Declined | There is no mandatory server (SKILL §12). The editor hands bytes to the host through the `save` and `export` events and the host decides where they go; a URL the editor posts to would put a server in the middle of a local-first product. |
| `editorConfig.createUrl` | — | Declined | Host navigation belongs to the host's page, which owns the chrome around the frame. |
| `editorConfig.saveAsUrl` | — | Declined | Same: the `export` event carries the bytes and the host chooses the destination. |
| `editorConfig.sharingSettingsUrl` | — | Declined | Sharing is the host's product. Ours has no account model to share against. |
| `editorConfig.fileChoiceUrl` | — | Gap — work named | Insert-image-from-host-storage needs a request/response event pair; not shipped. |
| `editorConfig.recent` | — | Declined | A list of the host's other documents inside our File page is the host's navigation drawn in our chrome. `can=-open` is how a host keeps the container on one document. |
| `editorConfig.templates` | — | Gap — work named | There is no template gallery yet; `can=-new` withholds the blank document instead. |
| `editorConfig.actionLink` | — | Gap — work named | Open-and-scroll-to-a-bookmark exists as a command (`reference.goToHeading`, bookmarks) but not as a boot parameter. A real gap, and chrome work rather than engine work. |
| `editorConfig.plugins` | — | Gap — work named | Deliberately after the command surface (docs/126): a plugin API built before the command contract stabilises is a second addressing scheme. `host_contract.mjs` is that surface, and a plugin registry layers on it. |
| `editorConfig.wopi` | — | Declined | WOPI is a server integration protocol; there is no mandatory server. |
| `customization.logo.image` | `brand.json mark` | Have it | A build-time artifact, not a runtime URL: the chrome must paint with the network off. |
| `customization.logo.imageDark` | `brand.json markDark` | Have it | — |
| `customization.logo.imageLight` | `brand.json mark` | Have it, differently | Two variants, not three: ours are light and dark, and their light header is our light theme. |
| `customization.logo.url` | `brand.json markHref` | Have it | — |
| `customization.logo.visible` | `brand.json mark:false / can=-branding` | Have it | Two spellings for two questions: the deployment has no mark, or this container shows none. |
| `customization.customer.name` | `brand.json customer.name` | Have it | — |
| `customization.customer.address` | `brand.json customer.address` | Have it | — |
| `customization.customer.mail` | `brand.json customer.mail` | Have it | — |
| `customization.customer.www` | `brand.json customer.www` | Have it | — |
| `customization.customer.phone` | `brand.json customer.phone` | Have it | — |
| `customization.customer.info` | `brand.json customer.info` | Have it | — |
| `customization.customer.logo` | `brand.json customer.logo` | Have it | Relative, like every other asset: a remote logo is a network dependency in the chrome. |
| `customization.customer.logoDark` | `brand.json customer.logoDark` | Have it | — |
| `customization.about` | — | Gap — work named | About and the shortcut reference are COMMAND rows (`help.about`, `help.shortcuts`), rendered from the command taxonomy into the File page and the menu bar. Withholding them is registry work rather than chrome, and a region that hid two of three Help rows would leave the third reachable from the palette — present-but-unreachable in reverse. |
| `customization.feedback.visible` | `brand.json feedback` | Have it | There is no feedback link unless the deployment configures one, so `false` is the default. |
| `customization.feedback.url` | `brand.json feedback.url` | Have it | Shown in About, with the host's own label — their language, not ours. |
| `customization.help` | `brand.json help.url` | Have it, differently | The host's help destination is configurable; hiding our own Help rows is not — see `customization.about`. |
| `customization.goback` | — | Declined | Their back button exists because their editor REPLACES the host's page — it is a full-window application that has to offer a way out. `<opendoc-editor>` is a region inside the host's own page, where the host's chrome already owns navigation, so a second Back inside the frame would be a control the host did not design pointing at a page they are already on. |
| `customization.close` | — | Gap — work named | The half of `goback` that is real for an embed is the opposite direction: a visitor inside the frame asking to leave. That is an editor control plus a `close` host event, and it is chrome work — reported rather than half-built. |
| `customization.review.hideReviewDisplay` | `chrome=-review` | Have it | The review-mode switcher is its own region, so a host can take the control without taking the band. |
| `customization.review.trackChanges` | `mode=commentor` | Have it, differently | A container opens in the strongest mode its grant allows (`editingModeFor`), so `commentor` opens in Suggesting. Opening an `edit` container in Suggesting is a preference the editor does not have a setting for yet. |
| `customization.review.showReviewChanges` | — | Gap — work named | `view.showChanges` is a command, not a stored setting, so there is nothing to preset. |
| `customization.review.reviewDisplay` | — | Gap — work named | Markup/original/final projection is designed (docs/133) and has no boot parameter. |
| `customization.review.hoverMode` | — | Declined | Balloons on hover fire on every pointer move over text; ours open on the change. |
| `customization.reviewPermissions` | — | Gap — work named | Per-group accept/reject needs identity; see `permissions.reviewGroups`. |
| `customization.anonymous` | `prefs={"user":…}` | Have it, differently | No anonymous-name prompt: a host that knows who is looking says so, and otherwise nobody is named. |
| `customization.layout.toolbar` | `chrome=-ribbon` | Have it | — |
| `customization.layout.toolbar.file` | `chrome=-band.file` | Have it | — |
| `customization.layout.toolbar.home` | `chrome=-band.home` | Have it | — |
| `customization.layout.toolbar.insert` | `chrome=-band.insert` | Have it | — |
| `customization.layout.toolbar.layout` | `chrome=-band.layout` | Have it | — |
| `customization.layout.toolbar.references` | `chrome=-band.references` | Have it | — |
| `customization.layout.toolbar.collaboration` | `chrome=-band.review` | Have it | Their Collaboration tab is our Review band. |
| `customization.layout.toolbar.view` | `chrome=-band.view` | Have it | — |
| `customization.layout.toolbar.draw` | — | Gap — work named | There is no Draw band: inking is not implemented. Shapes are on Insert. |
| `customization.layout.toolbar.protect` | — | Gap — work named | No Protect band; see `permissions.protect`. |
| `customization.layout.toolbar.plugins` | — | Gap — work named | No plugin surface yet; see `editorConfig.plugins`. |
| `customization.layout.header` | `chrome=-brand,-title,-state` | Have it, differently | Our top bar is three regions rather than one switch, which is finer: a host can keep the document name and drop the mark. There is no users list or avatar to hide. |
| `customization.layout.leftMenu` | `chrome=-rail` | Have it | — |
| `customization.layout.leftMenu.mode` | — | Gap — work named | The rail's open/closed state at boot is not a stored setting; `chrome=-rail` removes it entirely. |
| `customization.layout.rightMenu` | — | Gap — work named | THERE IS NO RIGHT PANEL. Object and paragraph properties are dialogs and a floating object bar, so there is no surface to hide — and hiding one that does not exist would be a configuration option that does nothing, which is the dead control rule in configuration form. |
| `customization.hideRightMenu` | — | Gap — work named | Same: no right panel exists to hide on first load. |
| `customization.layout.statusBar` | `chrome=-status` | Have it | — |
| `customization.layout.statusBar.actionStatus` | — | Have it, differently | The status bar is one region; its counts and chips are not separately withholdable. |
| `customization.toolbar` | `chrome=-ribbon` | Have it | — |
| `customization.leftMenu` | `chrome=-rail` | Have it | — |
| `customization.rightMenu` | — | Gap — work named | No right panel. |
| `customization.statusBar` | `chrome=-status` | Have it | — |
| `customization.hideRulers` | `chrome=-ruler` | Have it | — |
| `customization.toolbarHideFileName` | `chrome=-title` | Have it | — |
| `customization.comments` | `can=-comment` | Have it | — |
| `customization.chat` | — | Declined | See `permissions.chat`. |
| `customization.spellcheck` | `prefs={"spellcheck":…}` | Have it | — |
| `customization.features.spellcheck.mode` | `prefs={"spellcheck":…}` | Have it | — |
| `customization.features.spellcheck.change` | — | Gap — work named | Hiding the proofing switches entirely would be a region; there is none, because Settings is one region and splitting it per row is a redesign of the dialog. |
| `customization.uiTheme` | `prefs={"theme":…}` | Have it, differently | A DEFAULT, never a pin. ADR-039 refuses to let a deployment pin light/dark because that is a reader's preference about their own eyes; an opening position they can change in Settings takes nothing from them. |
| `customization.autosave` | `can=-autosave / autosave=0` | Have it | A capability rather than a preference, because it decides whether drafts are KEPT on this origin. |
| `customization.forcesave` | — | Declined | Forced save is a server round-trip; there is no mandatory server. |
| `customization.compactToolbar` | — | Gap — work named | The compact (Docs-shaped) toolbar exists and `view.compactRibbon` toggles it, but the choice is not a stored setting, so there is nothing for a boot parameter to write. One setting key away, and reported rather than faked. |
| `customization.compactHeader` | — | Gap — work named | Same as `compactToolbar`. |
| `customization.toolbarNoTabs` | — | Declined | A tabless ribbon is a different information architecture, not a setting: `docs/122` fixed the editor on ONE navigation axis at a time, and a ribbon with no tabs beside a menu bar is two. |
| `customization.features.tabStyle` | — | Declined | Fill-versus-line tabs is a restyle, and `docs/63` says to propose visual changes rather than make them. The white-label seam is tokens; geometry and chrome shape are not brand. |
| `customization.features.tabBackground` | — | Declined | Same as `tabStyle`. |
| `customization.features.featuresTips` | — | Declined | There are no what's-new tips to suppress, and adding some in order to suppress them is backwards. |
| `customization.suggestFeature` | — | Declined | No in-product feedback nag. `brand.json feedback.url` is a link the host owns. |
| `customization.zoom` | — | Gap — work named | Zoom is per session and not stored, so there is no default for a host to move. |
| `customization.unit` | — | Gap — work named | The dialogs and the ruler are inches only (`webapp/src/units.mjs`: twips and EMUs inside, inches at the field). Centimetres and points are a real gap and a real piece of work — every dialog parser, the ruler readout and the status bar — not a flag. |
| `customization.font` | — | Gap — work named | A default face and size for new documents is engine work: it is the Normal style. |
| `customization.pointerMode` | — | Gap — work named | There is no hand/pan tool; `webapp/src/pointer_cursor.mjs` reflects state, it does not offer a mode. |
| `customization.macros` | — | Declined | There is no macro engine and there is not going to be one on this schedule. A document format that can execute code in the reader's browser is the single largest attack surface an office suite has; `.docm` is already refused at open. So there is nothing to gate — which is a stronger answer than a policy flag. |
| `customization.macrosMode` | — | Declined | See `customization.macros`: no engine, so no warn/enable/disable policy. |
| `customization.plugins` | — | Gap — work named | See `editorConfig.plugins`. |
| `customization.mentionShare` | — | Gap — work named | Mentions need a user directory, which arrives with collaboration. |
| `customization.compatibleFeatures` | — | Declined | We do not have a lowest-common-denominator mode to switch into: unsupported document data is preserved or reported (SKILL §12), which is the same promise without a flag. |
| `customization.integrationMode` | — | Declined | Their flag turns off a scroll-into-view the embed does; ours never scrolls the host's page. |
| `customization.forceWesternFontSize` | — | Gap — work named | Chinese-locale font-size presentation; not implemented. |
| `customization.wordHeadingsColor` | — | Declined | A document's heading colour belongs to its style table, not to the host's configuration. |
| `customization.mobile` | — | Gap — work named | The phone layout is in flight (docs/148); its host switches are not designed yet. |
| `customization.hideNotes` | — | Another product | Presentations. |
| `customization.slidePlayerBackground` | — | Another product | Presentations. |
| `customization.showVerticalScroll` | — | Another product | Spreadsheets. |
| `customization.showHorizontalScroll` | — | Another product | Spreadsheets. |
| `customization.submitForm` | — | Another product | PDF forms. |
| `customization.startFillingForm` | — | Another product | PDF forms. |
| `customization.features.roles` | — | Another product | PDF form roles. |
<!-- @end option-map -->

## Their events, and ours

ONLYOFFICE list around forty `events`. Most of them are **requests to a server**:
`onRequestHistory`, `onRequestSaveAs`, `onRequestSharingSettings`, `onRequestRename`,
`onRequestUsers`, `onRequestCompareFile`, `onRequestReferenceData`. They exist
because their editor cannot open, save or version a file by itself — format I/O is
`x2t`, and `core/X2tConverter/build/` has only `Android/` and `Qt/`, no WASM build.
The host has to do those things for them.

Ours are seven, in `webapp/src/host_contract.mjs`: `ready`, `change`, `selection`,
`save`, `export`, `error`, `refusal`. The asymmetry is structural rather than a
gap — we are local by construction, so there is nothing to ask a host for — with
three real exceptions, all of them recorded in the table above:

* **`onRequestClose`** — a visitor inside the frame asking to leave. Real for an
  embed, and it needs an editor control as well as an event.
* **`onRequestInsertImage`** / `fileChoiceUrl` — inserting from the host's own
  storage. A request/response pair we do not have.
* **`onMakeActionLink`** / `actionLink` — open at a bookmark or a comment. The
  navigation exists as a command; the boot parameter does not.

Two things ours has that theirs does not, and both are in the same file:

* **A correlation id.** Their `_sendCommand` posts `{command, data}` and their
  `_onMessage` reads `{event, frameEditorId, data}`; there is no `rid` anywhere, so
  a host cannot learn whether what it asked for happened or was refused. Ours
  returns a result, and a refusal is part of it.
* **An origin contract.** Theirs posts to `"*"` under a literal
  `// TODO: specify explicit origin`, and their inbound filter carries
  `// TODO: check message origin`. Ours never posts to a wildcard, and `*` and
  `null` are both rejected as allowlist entries.

## What this does not do yet

Named here because understating is also false, and because each one is a row
somebody can pick up. Every one of them is `Gap — work named` in the table above.

* **`unit`** — centimetres and points. The dialogs and the ruler are inches only
  (`webapp/src/units.mjs`), so this is every dialog parser, the ruler readout and
  the status bar. Not a flag.
* **`permissions.copy`** — there is no clipboard gate. Adding one is a capability
  plus a choke point in the copy path.
* **The right panel** — there is none. Properties are dialogs and a floating object
  bar, so their `hideRightMenu` has no surface to hide, and offering the option
  would be a dead control in configuration form.
* **`compactToolbar`** — the compact bar exists and `view.compactRibbon` toggles it;
  the choice is not a stored setting, so there is nothing for a boot parameter to
  write. One setting key away.
* **`close`** — see the events section.
* **Per-group review and comment rights** (`reviewGroups`, `commentGroups`,
  `userInfoGroups`, `reviewPermissions`) — all need multi-user identity, which
  arrives with collaboration (ADR-033, `docs/107`).
* **Plugins** — deliberately after the command surface. A plugin API built before
  the command contract stabilises is a second addressing scheme, and
  `host_contract.mjs` is that surface.
* **`zoom`**, **`leftMenu.mode`**, **`review.reviewDisplay`**, **`mobile.*`** — each
  needs a stored value or a designed switch that does not exist yet.

## Related

* `docs/125` — the embed and host contract design.
* `docs/126` — the three-phase embeddability plan and the container policy.
* ADR-039 — white-labelling: the overridable token set, and why a host may pin the
  accent and may not pin light/dark.
