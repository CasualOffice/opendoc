# `@casualoffice/opendoc-embed`

The framework-free `<opendoc-editor>` custom element, and the capability
contract it resolves before first paint.

Apache-2.0. Nothing in here is gated on a licence check, and that is the point:
ONLYOFFICE's host-customization path early-returns when unlicensed
(`LayoutManager.js`, `if (!_licensed || !config) return;`), and the permissive
licence is the only structural advantage that cannot be copied.

## Commanding the editor

The element mounts an editor; the **host contract** is how you tell it to do
something and hear what it did. One schema, two transports — identical whether
you hold a reference or are across an origin boundary.

Across an origin, use the client. `editorOrigin` is required and may not be `"*"`:
the only default that would always work is a wildcard, and posting document
content to a wildcard target is not a decision a library should make for you.

```js
import { createHostClient } from "@casualoffice/opendoc-embed/client";

const editor = document.querySelector("opendoc-editor");
const client = createHostClient({
  frame: editor.frame,
  editorOrigin: "https://docs.example",
});

client.on("change", ({ detail }) => save(detail.revision, detail.dirty));
client.on("refusal", ({ detail }) => report(detail.code, detail.message));

const result = await client.execute("format.bold");
if (!result.ok) {
  // Branch on the CODE. The message is localised and belongs to the editor.
  if (result.refusal.code === "capability-withheld") askForEditRights();
}
```

Same origin, you can hold the session directly — `window.opendoc` inside the
frame — and it answers exactly the same thing:

```js
const result = await editor.frame.contentWindow.opendoc.execute("format.bold");
```

`describe()` enumerates every command with its requirement and its current
state, so a host never hard-codes a command list. `query(id)` answers for one.
`change` carries a revision handle and a dirty flag, never a snapshot.

The editor accepts messages from its own origin, and from any origin the
deployment names with `?hostOrigin=https://your.app`. Anything else is dropped in
silence.

Capabilities gate the API, not only the chrome: a `readonly` embed refuses an
editing command **before** it reaches the engine, and says so in the result.

## Install

The package is not published to a registry. Build and install the verified
repository tarball for evaluation:

```sh
cd packages/opendoc-embed
npm pack
npm install ./casualoffice-opendoc-embed-0.1.0.tgz
```

Zero runtime dependencies. Works in React, Vue, Svelte and plain HTML without a
binding layer — that is why it is a custom element and not three wrappers.
Framework bindings are deliberately not shipped.

## Use

```html
<script type="module">
  import "@casualoffice/opendoc-embed/define";
</script>

<opendoc-editor
  mode="commentor"
  can="-download"
  chrome="-brand"
  editor-src="/opendoc/editor.html"
  frame-title="Contract preview"
></opendoc-editor>
```

That is a commentor who may not take the file away, in a frame carrying none of
our brand — a real configuration, expressed declaratively. `can` and `chrome`
only ever **narrow** the role, never widen it, whatever you put in them.

Or keep the registration explicit:

```js
import { defineOpenDocEditor } from "@casualoffice/opendoc-embed";

defineOpenDocEditor(); // idempotent; returns whether this call registered it
```

The element needs the editor's own static files (`editor.html` and the
WebAssembly bundle beside it) served from somewhere you control. Point
`editor-src` at them. This package is the mount and the capability contract; it
does not carry the engine.

## Attributes

| Attribute | Meaning |
| --- | --- |
| `mode` | One of the five roles, or a legacy preset name. Absent means the framed default, which an element-mounted editor always is — so absent is never `standalone`. |
| `can` | Capabilities to **withhold** from the role: `"-download"`, `"-print,-download"`. Narrows only. |
| `chrome` | Chrome regions to withhold, by the same rule: `"-brand,-rail"`. Withholding `ribbon` withholds its bands too. |
| `editor-src` | Where `editor.html` is. May carry your own query (`?blank=1`); `mode` is merged into it rather than appended twice, and `can`/`chrome` are unioned with any the query already carries. |
| `frame-title` | The frame's accessible name, in **your** page's language. |

`can` and `chrome` are attributes, rather than something you hand-write into
`editor-src`'s query, because the per-capability list — not the role — is the
unit of this model: "commentor, but without download" is a real configuration and
not a rung on a ladder. A role is only a name for a subset, and nothing
downstream branches on one.

Both are **narrow-only**, in one direction, always: a list can take a capability
away from a role and can never give one back. An unknown entry is dropped rather
than refused, and dropping narrows nothing, so a typo cannot widen the result.
That is what makes there be exactly one direction to audit — and it is why a
value on the attribute is *unioned* with one already in `editor-src`, rather than
replacing it. Replacing could hand back something the query had removed.

It is `mode`, not `role`, because `role` is a global ARIA attribute:
`role="readonly"` would publish an invalid ARIA role to every assistive
technology on the page. `mode` is also the spelling the URL parameter uses, so
there is one vocabulary rather than two.

`frame-title` has no default. An iframe with no accessible name is an axe
`frame-title` violation, and this package has no business inventing English for
your page — you get a console warning, once, and no label.

## Roles

Five presets over nine capabilities, in strictly increasing order of what they
grant. The chain is monotone, and that is asserted rather than described: a
"higher" role quietly losing something a lower one had is the classic
permissions defect.

| Role | Capabilities | For |
| --- | --- | --- |
| `preview` | — | Look, and nothing else. |
| `readonly` | `print` | A published document: read it, take a paper copy. |
| `commentor` | `print` `download` `comment` | Annotate and suggest; do not change the document outright. |
| `edit` | `print` `download` `comment` `save` `edit` `autosave` | The reason you embedded an editor. |
| `owner` | all nine (adds `open` `new` `branding`) | The page is yours. |

`readonly` withholds `download` deliberately: a reader who can download holds
the file, which is a different grant from being allowed to look at it.

`standalone` and `viewer` are exact aliases of `owner` and `readonly`.
`embedded` is `edit` minus `autosave`, and is what a page gets when it turns out
to be framed and nobody asked for a mode.

## How a mode is enforced

Three layers, in order of how hard they are to defeat.

1. **The browser.** The element derives the iframe's `sandbox` tokens from the
   capability set. A frame without `allow-downloads` cannot start a download
   however its chrome or its engine is persuaded.
2. **The engine.** A capability set maps onto the editor's existing three review
   modes — `edit` → Editing, `comment` → Suggesting, neither → Viewing. Viewing
   is fully read-only at a single fail-closed choke point: every mutation path
   is refused there rather than relying on any individual control being
   disabled, so a chrome defeated from devtools still meets it.
3. **The chrome.** A withheld capability disables its control **with a reason**.
   It never hides it: a control that vanishes teaches nothing, and a product
   that silently lacks a feature is indistinguishable from one that is broken.

The mode and both withhold lists are written into the frame's URL **before the
frame is attached**, so there is no window in which the editor is standalone and
is then told otherwise. Setting `mode`, `can` or `chrome` afterwards therefore
remounts. A permission handed over after the document is on screen was never a
permission.

## Properties and events

```js
const editor = document.querySelector("opendoc-editor");

editor.capabilities; // readonly ["print"]
editor.regions;      // the chrome regions this container was offered
editor.editingMode;  // "viewing"
editor.sandbox;      // ["allow-scripts", "allow-same-origin", "allow-modals"]
editor.can("edit");  // false
editor.frame;        // the live HTMLIFrameElement, or null

editor.addEventListener("opendoc-capabilities", (event) => {
  console.log(event.detail.editingMode, event.detail.capabilities);
});
```

`can` and `chrome` have no reflecting property, deliberately: `can(capability)`
is already the method you ask "was I granted this", and a `can` accessor would
replace it. Set them as attributes — `setAttribute("can", "-download")`, or in
your markup — and read back what they resolved to from `capabilities`, `regions`
and `can(name)`, which is the more useful question anyway.

`opendoc-capabilities` fires once per mount. It is the capability contract
announcing itself. The editor also exposes the typed host command/event contract
described above; those events report document/session behavior without changing
the capability event's meaning.

## What this does not do yet

Stated plainly rather than left to be discovered:

- **No way to hand it your document.** `editor-src` takes the editor's own
  query parameters; there is no host-supplied document URL or byte array yet.
- **No real-time collaboration.** There is no collaboration adapter, relay,
  remote-presence surface, or multiplayer support claim. Docs 143–144 are an
  experimental architecture and phased checklist only.
- **White-labelling is build-time, not a runtime remote theme service.** The host
  may compose supported chrome regions and build theme/brand assets; runtime
  arbitrary CSS injection is deliberately not a contract.
- **No framework bindings**, by choice — see above.
- **Not published.** This package is built and installable from a tarball
  (`npm pack`); no CI job publishes it to a registry yet.
