# 183 — The AI provider contract and the plugin platform

**Status:** Designed 2026-10-30; P1 (provider contract and adapters) and P2 (plugin runtime)
built on `program/ai-plugins`. P3 onward (the assistant and the first-party plugins) waits
for the owner's review of §11–§13 (owner instruction, 2026-10-30: "research what users
generally get" and discuss the features and the UI before they are built).
**Decides:** ADR-077 (proposed). **Rows:** `docs/109` RM-50 (provider contract), RM-51
(plugin platform), RM-52 (assistant plugin), RM-53 (further first-party plugins), RM-54 (RAG,
MCP client and the agent loop).
**Supersedes:** ADR-009's web half ("trusted native plugins in v1; later sandboxed"): on the
web a plugin is never in-page code. ADR-009 still describes native Rust embedders, who link
the engine and are trusted by construction. **Amends:** `178` §4.4 ("OpenDoc does not
embed, store vectors or call a model") and §6.4 (sampling): the editor now calls a model,
but only one the user or the host configured, over an open wire format, and only on a
person's request. **Builds on:** `181` and ADR-075 (the one capability registry), `178` §9
and the `editor.ai` private preview, ADR-036 and `149` (the host contract), `175` (the
directory seam, whose shape the host provider channel copies), ADR-030 I1–I4.

**Owner decisions, 2026-10-30 (final):**

1. AI uses open standards, never a home-grown protocol: the OpenAI-compatible Chat
   Completions wire format, plus Anthropic Messages. Standalone: bring your own key, stored
   on the device only. Embedded: a host-contract `aiProvider` callback whose request and
   response ARE that JSON, so no key reaches the browser. MCP sampling when an MCP host lends
   its model. Agents connecting to us: the existing MCP and WebMCP. No provider configured
   means no AI and no network calls.
2. Plugins run as sandboxed iframes over a postMessage RPC that exposes the same registry
   API as MCP and WebMCP. A manifest declares id, version, entry, permissions and UI
   contributions. Never in-page code. Macros stay declined.
3. The in-editor assistant is a first-party plugin on the public plugin API, which proves the
   API is complete. Every edit goes through the private preview, then tracked changes by
   "Casual AI". The UI states which endpoint receives the document text.

---

## 1. The patterns, named before any design

| Problem | Known pattern | Precedent |
| --- | --- | --- |
| One client, many model vendors | **Adapter over a de-facto standard wire format**; the canonical form is the most widely implemented one | OpenAI Chat Completions is what Azure OpenAI, Mistral, OpenRouter, Groq, Together, vLLM, LM Studio, Ollama and Gemini's OpenAI-compatible endpoint all accept; LiteLLM and the Vercel AI SDK normalise to it |
| A key must not reach the browser in a hosted product | **Backend-for-frontend proxy**: the host forwards an already-formed request to its own backend | The directory seam (`175` §7): a bounded question to the host, answered by `qid` |
| Untrusted extension code inside a document editor | **Capability-based sandbox**: an opaque-origin iframe, a CSP the extension cannot loosen, and a message port as the only channel | VS Code webviews, Figma plugins (iframe UI, postMessage), Google Workspace add-ons (iframe sidebar over `google.script.run`), Office Add-ins (iframe task pane, manifest permissions) |
| The extension API must not become a second addressing scheme | **One registry, many front doors** (`181` §1): the plugin RPC is a transport over the registry's tools, not a new vocabulary | MCP `tools/list` / `tools/call`; WebMCP `registerTool` |
| A plugin asks for more than it needs | **Least privilege, declared up front and granted at install**, re-asked when the declaration widens | Android runtime permissions; browser extension manifests; Office Add-in `Permissions` |
| Model output must not edit the document unseen | **Human in the loop over a staged change**: preview, then tracked changes | `181` §4.4 (built); Docs and Word assistants show a proposal card |
| Untrusted text inside a prompt | **Spotlighting / data-marking**: document text travels in delimited data blocks the system prompt names as data | Microsoft, "Defending against indirect prompt injection attacks with spotlighting" (2024); OWASP LLM01 |

No pattern is invented. What is specific to us is that the plugin RPC and the MCP and WebMCP
surfaces are generated from the same tool table, so a plugin can never reach more than an
agent can, and the grant it holds is the intersection of three authorities (§6.3).

## 2. Where it sits

```
 Plugin iframe (sandbox="allow-scripts", opaque origin, CSP connect-src per grant)
      │  MessagePort, JSON-RPC 2.0                 Host page (embedded)
      ▼                                                  ▲ ai-provider event / ai-provider.answer
 plugin_host.mjs ── permission gate ──► tools (= ai_webmcp toolsFor) ──► editor.ai ──► engine
      │                                  commands by id ──► host session execute (gated)
      └── ai/complete ──► ai_provider.mjs ──► adapter: openai | anthropic | host | mcp-sampling
                                                    │
                                     BYOK endpoint (standalone) — the only network hop,
                                     shown to the person before anything is sent
```

Everything a plugin does reaches the document through `editor.ai` (reads and the private
preview) or through the host session's gated `execute`, which are the two doors a host
already has. The plugin layer adds no mutation path (ADR-030 I1) and no operation (I2).

## 3. The OpenDoc AI Provider Contract, version 1

### 3.1 Principle

The contract is **not a new protocol**. A request is an OpenAI Chat Completions request
body; a response is a Chat Completions response; a stream is a sequence of Chat Completions
chunks; an error is the OpenAI error object. Our only additions travel **beside** the body,
never inside it: `purpose` (what the person asked for, for the host's audit) and the
cancellation signal. A host whose backend already proxies OpenAI-compatible traffic forwards
the body unchanged.

Specifications, read on 2026-10-30:

- OpenAI Chat Completions: <https://platform.openai.com/docs/api-reference/chat/create>;
  streaming chunk object: <https://platform.openai.com/docs/api-reference/chat-streaming>;
  error codes: <https://platform.openai.com/docs/guides/error-codes>.
- Anthropic Messages: <https://docs.anthropic.com/en/api/messages>; streaming events:
  <https://docs.anthropic.com/en/api/messages-streaming>; CORS from a browser needs the
  `anthropic-dangerous-direct-browser-access: true` header.
- MCP sampling, revision 2026-07-28:
  <https://modelcontextprotocol.io/specification/2026-07-28/client/sampling>. **Deprecated
  in that revision** (SEP-2577), kept in the specification for at least twelve months; new
  implementations "SHOULD NOT adopt it". §3.7 says what we do about that.
- MCP tools: <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>.
- WebMCP, Draft Community Group Report: <https://webmachinelearning.github.io/webmcp/>.
- OpenAI-compatible endpoints we name as targets: Azure OpenAI
  (<https://learn.microsoft.com/azure/ai-services/openai/reference>), Gemini's OpenAI
  compatibility (<https://ai.google.dev/gemini-api/docs/openai>), Mistral
  (<https://docs.mistral.ai/api/>), OpenRouter (<https://openrouter.ai/docs/api-reference/overview>),
  Ollama (<https://github.com/ollama/ollama/blob/main/docs/openai.md>), vLLM
  (<https://docs.vllm.ai/en/latest/serving/openai_compatible_server.html>), LM Studio
  (<https://lmstudio.ai/docs/app/api/endpoints/openai>).

### 3.2 The request

The JSON Schema is `webapp/schemas/ai-provider-request.schema.json` (2020-12). It is the
**subset of Chat Completions the editor sends**; a host may accept more, the editor never
sends more.

| Field | Type | Rule |
| --- | --- | --- |
| `model` | string | The configured model id; the host may override it |
| `messages` | array, 1–256 | `{role: "system" \| "user" \| "assistant" \| "tool", content: string \| [{type: "text", text}], name?, tool_call_id?, tool_calls?}` |
| `max_completion_tokens` | integer 1–32768 | Always present; the editor's output ceiling (§3.6) |
| `temperature` | number 0–2 | Optional |
| `stop` | array of ≤4 strings | Optional |
| `stream` | boolean | `true` when the caller wants chunks |
| `stream_options` | `{include_usage: true}` | Only with `stream` |
| `tools`, `tool_choice` | Chat Completions function tools | Only for the agent loop (§12.3) |
| `response_format` | `{type: "text" \| "json_object" \| "json_schema", json_schema?}` | Optional |

`max_tokens` (the older name) is never sent; the OpenAI adapter writes
`max_completion_tokens`, and for endpoints that reject it (some local servers) the settings
carry a compatibility switch that renames it to `max_tokens`. Unknown fields are rejected by
`checkRequest` before anything leaves, so a plugin cannot smuggle a provider-specific field
past the contract.

### 3.3 The response, the stream, the error

- **Response** (`ai-provider-response.schema.json`): `{id, object: "chat.completion",
  created, model, choices: [{index, message: {role: "assistant", content, tool_calls?},
  finish_reason}], usage?: {prompt_tokens, completion_tokens, total_tokens}}`.
  `finish_reason` ∈ `stop`, `length`, `content_filter`, `tool_calls`.
- **Stream** (`ai-provider-chunk.schema.json`): `chat.completion.chunk` objects with
  `choices[0].delta.{role?, content?, tool_calls?}` and a final `finish_reason`, optionally a
  last chunk with `usage`. Over HTTP this is SSE (`data: {...}` lines ending `data: [DONE]`);
  over the host callback and the plugin RPC it is the chunk objects themselves. The caller
  always receives the **assembled response** at the end as well, so a caller that ignores
  chunks is correct.
- **Error**: the OpenAI error object `{error: {message, type, code, param}}`, plus our
  normalised `code`, which is what the UI and a plugin branch on:

| Code | From |
| --- | --- |
| `provider-unconfigured` | No provider (the default). Nothing was sent |
| `provider-disabled` | The host or admin switched AI off, or this feature off (§13.4) |
| `bad-request` | The request failed `checkRequest`, or the endpoint answered 400 |
| `context-too-long` | Over the input budget before sending, or the endpoint's `context_length_exceeded` |
| `over-budget` | The session's token or request budget is spent (§3.6) |
| `auth-failed` | 401 or 403 |
| `rate-limited` | 429; `retryAfter` carried when the endpoint sent `Retry-After` |
| `content-filtered` | `finish_reason: content_filter`, or a provider refusal |
| `server-error` | 5xx, Anthropic `overloaded_error` |
| `network` | `fetch` rejected: offline, DNS, or CORS. The sentence names CORS, because a local server without `OLLAMA_ORIGINS` is the usual cause |
| `timeout` | No first byte within 30 s, or no chunk within 60 s |
| `cancelled` | The person pressed Stop, or the caller aborted |
| `bad-response` | The answer was not a Chat Completions object |
| `host-refused` | The host callback answered with an error of its own |

### 3.4 Streaming and cancellation

Every call takes an `AbortSignal`. HTTP adapters pass it to `fetch`, so Stop closes the
connection and the provider stops billing (OpenAI and Anthropic both stop generation on
disconnect). The host channel sends an `ai-provider` event `{qid, kind: "cancel"}`; the plugin
RPC sends `ai/cancel`. A cancelled call resolves `{ok: false, error: {code: "cancelled"}}`
with whatever text had arrived in `partial`, so the UI can keep or drop it.

### 3.5 Adapters

| Adapter | Wire | Notes |
| --- | --- | --- |
| `openai` | `POST {baseUrl}/chat/completions`, `Authorization: Bearer <key>` (absent for a keyless local server); Azure: `api-key` header and `?api-version=` | Body passed through after `checkRequest` |
| `anthropic` | `POST {baseUrl}/v1/messages`, `x-api-key`, `anthropic-version: 2023-06-01`, `anthropic-dangerous-direct-browser-access: true` | `system` messages lifted to `system`; `max_completion_tokens` → `max_tokens` (required); `stop` → `stop_sequences`; `stop_reason` `end_turn`/`stop_sequence` → `stop`, `max_tokens` → `length`, `tool_use` → `tool_calls`, `refusal` → `content_filter`; SSE `content_block_delta.text_delta` → chunk `delta.content`; `usage.input_tokens/output_tokens` → `prompt_tokens/completion_tokens` |
| `host` | The host's `aiProvider(request, {signal, onChunk, purpose})`, in process or over `postMessage` (§3.8) | The body is passed as is; the host's answer is checked by `checkResponse` like any other |
| `mcp-sampling` | `sampling/createMessage` through a requester the MCP host supplies | Text-only; `messages` mapped to `{role, content: {type: "text"}}`, system messages to `systemPrompt`, `max_completion_tokens` → `maxTokens`, `stop` → `stopSequences`; result `stopReason` `endTurn`/`stopSequence` → `stop`, `maxTokens` → `length`, `toolUse` → `tool_calls`. No streaming: one chunk at the end |

### 3.6 Token limits and budgets

- **Input:** estimated before sending with `178` §4.5's declared heuristic (UTF-8 bytes ÷ 4,
  rounded up, `estimator: "bytes/4"`), against the configured context window (default
  32,000). Over it is `context-too-long`, refused locally, nothing sent. The bound errs
  towards sending less.
- **Output:** `max_completion_tokens` is always set: the caller's value, clamped to the
  configured ceiling (default 4,096).
- **Session budget:** a per-page ceiling on requests (default 200) and on estimated tokens
  (default 2,000,000), configurable by the host. Spent is `over-budget`. A runaway plugin or
  agent loop cannot spend unbounded money.
- Usage the endpoint reports is preferred over the estimate wherever it is present.

### 3.7 MCP sampling, honestly

The owner asked for sampling "when an MCP host lends its model". Revision 2026-07-28
deprecates it; `178` §6.4 had decided the MCP *server* never samples. Both stand: the
`mcp-sampling` adapter is a pure mapping the editor uses only when an MCP host hands it a
requester (an editor running inside an MCP host's UI, or a desktop shell bridging one). It is
not used by `opendoc-mcp`. When the feature is removed from MCP, the adapter is removed with
it; the contract does not change, because the adapter is behind it.

### 3.8 Resolution and the host channel

The provider is chosen once per call, strongest first:

1. **Host, in process:** `window.opendoc.aiProvider = async (request, {signal, onChunk,
   purpose}) => response` (and the `<opendoc-editor>` element's `aiProvider` property).
2. **Host, over `postMessage`:** the frame was opened with `?aiProvider=host` (the element
   writes it when its property is set). The editor emits the `ai-provider` event `{qid, kind:
   "request", request, purpose}` and the host answers with the `ai-provider.answer` request
   `{qid, kind: "chunk" | "result" | "error", chunk? | response? | error?}`; `{qid, kind:
   "cancel"}` withdraws a request. The same correlation-by-`qid` pattern as `directory-query`
   (`175` §7.2), with a 120 s ceiling per request.
3. **MCP sampling**, when a requester was supplied.
4. **Bring your own key**, standalone only: the editor is not framed and the host config did
   not say otherwise.
5. **None.** No AI command is enabled (each shows its reason), and the editor makes **no
   network call**.

`?aiProvider=off` withdraws every provider, BYOK included, for a host that wants no AI.
A framed editor never offers BYOK: a key typed into someone else's product would be stored on
an origin that product does not control, and the host has its own channel.

### 3.9 Bring your own key

- **Settings ▸ AI provider…** (and the palette's "AI provider settings"): a preset (OpenAI,
  Azure OpenAI, Anthropic, Google Gemini, Mistral, OpenRouter, Ollama, LM Studio, vLLM,
  other OpenAI-compatible), the base URL, the model, the key, and **Test connection**, which
  sends one fixed one-word prompt and shows the endpoint's answer or the normalised error.
- **Stored on this device only**, in this origin's `localStorage` under
  `opendoc.ai.provider`, never in a document, a draft, a version, a room or a log, and never
  sent anywhere but the configured endpoint. **Forget** deletes it. Storage that throws
  (private windows) leaves the key for this page only and says so.
- **The disclosure.** Settings and every AI surface name the receiving endpoint's origin
  ("Text you send goes to api.openai.com"), or "your organisation's service" for a host
  provider, before anything is sent. A non-HTTPS endpoint is accepted only for loopback
  (`localhost`, `127.0.0.1`, `[::1]`), for local models.
- Honest limit: `localStorage` is readable by any script on this origin. Our origin runs no
  third-party script, plugins run on an opaque origin and cannot read it (§7), and the key is
  never echoed back into the form (the field shows "saved"). An attacker with script on our
  origin could read it; so could they read the document. A host product uses the host
  channel, which keeps keys off the device entirely.

## 4. The plugin manifest

JSON Schema: `webapp/schemas/plugin-manifest.schema.json`. Example:

```json
{
  "manifestVersion": 1,
  "id": "org.casualoffice.languagetool",
  "name": "LanguageTool",
  "version": "1.0.0",
  "description": "Grammar and style checking against your LanguageTool server.",
  "publisher": "Casual Office",
  "entry": "index.html",
  "icon": "icon.svg",
  "permissions": ["read", "suggest", "network"],
  "network": ["https://languagetool.example.org"],
  "contributes": {
    "commands": [{ "id": "check", "title": "Check grammar" }],
    "ribbon": [{ "command": "check", "icon": "spellcheck" }],
    "panel": { "title": "Grammar", "width": 320 },
    "contextMenu": [{ "command": "check", "when": "selection" }],
    "palette": [{ "command": "check" }]
  },
  "locales": { "de": { "name": "LanguageTool", "commands.check": "Grammatik prüfen" } }
}
```

| Field | Rule |
| --- | --- |
| `id` | Reverse-DNS, `^[a-z0-9]+(\.[a-z0-9-]+)+$`, ≤ 128 bytes; stable forever |
| `version` | SemVer; a version that **adds** a permission re-asks the grant |
| `entry` | An HTML page, relative to the manifest or absolute `https:` (loopback `http:` allowed). Fetched by the editor (CORS needed across origins), ≤ 1 MiB |
| `permissions` | Subset of `read`, `comment`, `suggest`, `edit`, `network`, `ai` (§6) |
| `network` | Origins the plugin may `fetch`; required with `network`, refused without it; no wildcards |
| `contributes.commands` | ≤ 32; `id` `^[a-z][a-zA-Z0-9-]{0,47}$`; becomes editor command `plugin.<pluginId>.<id>` |
| `contributes.ribbon` | Items in the one **Add-ins** menu button (§8); never a new ribbon button per plugin |
| `contributes.panel` | One side panel, 240–480 px wide |
| `contributes.contextMenu` | `when`: `always`, `selection`, `caret` |
| `contributes.palette` | Command ids offered in the palette (every command is, by default) |
| `locales` | Per-locale `name`, `description` and `commands.<id>` titles; English otherwise |

Unknown fields are refused, not ignored: a manifest that says something we do not
understand may be asking for something we would not grant.

## 5. The RPC protocol

JSON-RPC 2.0 (<https://www.jsonrpc.org/specification>) over a **`MessagePort`**. Method names
and result shapes follow MCP where the operation is MCP's (`tools/list`, `tools/call`), so a
plugin author who knows MCP knows this.

**Handshake.** The editor builds the frame (§7), waits for its first `load`, and transfers one
end of a `MessageChannel` with `{opendocPlugin: 1, type: "connect"}`. From then on the port is
the only channel: the editor ignores every `window` message from the frame, so no other frame
can speak for a plugin and the plugin cannot reach the host contract's `postMessage` bridge.
A second `load` (the plugin navigated itself) disposes the plugin and says so.

**Plugin → editor (requests):**

| Method | Needs | Result |
| --- | --- | --- |
| `initialize` | — | `{protocolVersion: 1, plugin: {id, version}, permissions: granted[], access, locale, direction, theme, provider: {available, disclosure}}` |
| `tools/list` | `read` | MCP-shaped tool descriptors, exactly `ai_webmcp.toolsFor(grant)` for the plugin's effective grant |
| `tools/call` `{name, arguments}` | per tool | The `editor.ai` answer: reads, or `{status: "awaiting-person", previewId, preview}` for edits |
| `selection/get` | `read` | `{hasRange, text, paragraphs: [{id, text}], truncated}` bounded to 16 KiB |
| `commands/list` | — | Editor command ids the grant admits (§6.4) |
| `commands/run` `{id}` | §6.4 | The host session's `execute` result |
| `ai/complete` `{request, stream?}` | `ai` | Provider response; chunks as `ai/chunk` notifications |
| `ai/cancel` `{requestId}` | `ai` | — |
| `ui/notify` `{message, kind}` | — | Status line, ≤ 200 characters, prefixed with the plugin's name |
| `ui/panel` `{open: bool}` | panel declared | — |
| `storage/get`, `storage/set` | — | Per-plugin key/value, ≤ 256 KiB in total, this device only |

**Editor → plugin (notifications):** `contribution/invoked {command, context}`,
`selection/changed {hasRange}` (debounced 250 ms, no text), `document/changed {revision}`
(with `read`), `theme/changed`, `locale/changed`, `ai/chunk {requestId, chunk}`,
`plugin/closing`.

**Errors** are JSON-RPC errors with our codes in `data.code`: `permission-denied` (with
`data.permission`), `unknown-method`, `bad-request`, `rate-limited` (more than 20 requests
per second, or 4 concurrent `ai/complete`), plus every provider code of §3.3 and every
`editor.ai` refusal code.

**Bounds.** A message larger than 4 MiB is refused. 64 requests may be in flight per plugin.
Everything is a value; nothing throws across the port.

## 6. Permissions

### 6.1 The six

| Permission | Allows | Grant prompt says |
| --- | --- | --- |
| `read` | `tools/list`, the read tools, `selection/get`, `document/changed` | "Read this document" |
| `comment` | `add_comment` and comment plans, through the private preview | "Propose comments" |
| `suggest` | Trackable edit plans, through the private preview, as suggestions | "Propose changes as suggestions" |
| `edit` | Direct edit plans (only where the host granted direct edits), still through the preview; mutating commands by id | "Change this document" |
| `network` | `fetch` to the declared origins only | "Connect to: languagetool.example.org" |
| `ai` | `ai/complete` through the configured provider; never the key or the endpoint URL | "Use your AI provider (text goes to api.openai.com)" |

### 6.2 What no permission grants

Accepting or rejecting tracked changes, deciding a preview, saving, downloading, printing,
opening or creating documents, changing the identity, the room or the grant, reading
another plugin's storage, reaching the host page, or running code in the editor's origin.
A person decides a preview; a plugin only proposes.

### 6.3 The effective grant is an intersection

`effective = manifest.permissions ∩ (host grant | the person's grant) ∩ what the host
contract allows this mount`. The third term is `editor.ai`'s access (`aiAccessFor`): a
`suggest` plugin on a `readonly` mount gets reads only, and the engine still refuses at the
choke point whatever the editor decided (`178` §5.1). The plugin learns its effective grant
from `initialize`.

### 6.4 Commands by id

`commands/list` and `commands/run` reach the host session's `execute`, so a plugin can do no
more than the host itself. Within that: a contract row requiring nothing (view, navigation,
zoom) needs no permission; `mutate`, `edit` or `comment` rows need the `edit` or `comment`
permission; `save`, `download`, `print`, `open`, `new` and `autosave` rows are never
offered to a plugin. Plugin and AI commands are never runnable by a plugin (no plugin drives
another). A mutating command lands as the person's own edit, exactly as a host's `execute`
does; that is why it needs `edit` and why the prompt says "Change this document".

### 6.5 Where grants come from

- **Host-listed plugins** (`?plugins=<catalogue URL>`, §9): the host's catalogue says what
  it grants each plugin; no prompt, because the host is the administrator.
- **First-party catalogue** (`webapp/plugins/catalogue.json`), standalone: shown in
  **Add-ins ▸ Manage add-ins…**; enabling one shows the grant prompt.
- **Install from a manifest URL**, standalone and only when the host did not withhold it: the
  grant prompt names the entry origin and each permission in a sentence.
- Grants are stored per `id` with the granted set, in `localStorage`
  (`opendoc.plugins.grants`); revoking is one click; a new version that adds a permission
  shows the prompt again for the addition only.

## 7. The sandbox

- `<iframe sandbox="allow-scripts">` and nothing else: no `allow-same-origin` (the frame has
  an opaque origin and cannot read our storage, cookies or DOM), no `allow-popups`, no
  `allow-forms`, no `allow-top-navigation`, no `allow-modals`, no `allow-downloads`.
  `allow="..."` grants no feature (camera, microphone, clipboard, geolocation).
- The entry HTML is fetched by the editor and loaded as `srcdoc` with two things prepended
  to `<head>`: `<base href="{entry URL}">`, so its relative scripts and styles load from its
  own origin, and a `Content-Security-Policy` meta that a later policy can only tighten:
  `default-src 'none'; script-src {entry origin} 'unsafe-inline'; style-src {entry origin}
  'unsafe-inline'; img-src {entry origin} data: blob:; font-src {entry origin};
  connect-src {declared network origins, or 'none'}; frame-src 'none'; form-action 'none';
  base-uri {entry origin}`.
- The frame's only channel is the port (§5).
- A plugin is disposed on `plugin/closing` timeouts (2 s), on a second `load`, when its
  grant is revoked, and on `pagehide`.

## 8. UI contributions

Designed from Word and Docs first: Word puts one **Add-ins** button on Home that opens the
add-in list; Docs has one **Extensions** menu; ONLYOFFICE has a Plugins tab. A button per
plugin is how ribbons overflow, so:

- **One Add-ins menu button** at the right end of the Review band (fixed width, whatever the
  number of plugins): each plugin's `ribbon` items, then **Manage add-ins…**. Review, not
  Home, because every first-party plugin planned is proofing or review work, and Home's width
  budget belongs to formatting.
- **Side panel**: one at a time, in the right-hand panel column the review pane uses,
  titled with the plugin's name, with a close button that returns focus to what opened it.
- **Context menu**: `plugin.<id>.<command>` rows with `contextMenu: true`, shown by `when`.
- **Palette**: every plugin command, labelled "Plugin name: Command title".
- Every command is therefore on at least two surfaces (SKILL §10). A command whose plugin
  has not loaded, or whose permission was revoked, is present and disabled with the reason.
- **Host contract:** plugin commands are one command family, `plugin.`, requiring nothing of
  the host's grant (the plugin's own grant governs what it can do); `plugins.manage` and
  `ai.providerSettings` are exact rows requiring nothing.

## 9. Host configuration

| Input | Values | Meaning |
| --- | --- | --- |
| `?aiProvider=` / element `ai-provider` attribute and `aiProvider` property | `host`, `off` | §3.8 |
| `?plugins=` / element `plugins` attribute | a catalogue URL, or `none` | The host's plugin list; `none` withdraws every plugin, first-party included |
| catalogue JSON | `{catalogueVersion: 1, plugins: [{manifest, grant[], enabled}], allowInstall, allowFirstParty}` | Fetched once at boot; same-origin with the editor or CORS-enabled |

All three are read by `hostConfig()`, the one configuration channel, and reported back on
`ai.access()` and in the `ready` event's capabilities.

## 10. Threat model

| # | Threat | Mitigation | Residual |
| --- | --- | --- | --- |
| T1 | A plugin reads our origin's storage (BYOK key, drafts) | Opaque origin (no `allow-same-origin`) | None known |
| T2 | A plugin exfiltrates document text without `network` | CSP `connect-src 'none'`, `img-src` limited, `form-action 'none'`, no popups | **Self-navigation**: a frame may set its own `location` to a URL carrying data; CSP cannot block it (`navigate-to` was never shipped). The editor detects the second `load` and disposes the plugin, but the request has left. So the prompt for `read` without `network` says "can read this document", never "cannot send it anywhere", and third-party plugins with `read` are only installable where the host allows installs |
| T3 | A plugin edits the document unseen | Edits only through the private preview; a person accepts; tracked by default | A mutating command by id (`edit`) lands as the person's edit; gated by the `edit` permission and its prompt |
| T4 | A plugin escalates beyond its grant | Effective grant is an intersection checked on every call; the engine re-checks at the choke point | None known |
| T5 | Prompt injection from document text | Document text sent in delimited data blocks; the system prompt says text inside is data; the model's output only *proposes*; tool calls from the model are limited to read tools and `propose_edits` | A model can still be misled into a bad proposal; a person reviews every one |
| T6 | Key leakage | Key never in a document, log, room or plugin; plugins call `ai/complete` and never see it; host mode keeps it off the device | Script on our origin (§3.9) |
| T7 | Runaway spend | Session request and token budgets; `max_completion_tokens` always set; per-plugin rate limits | None known |
| T8 | A malicious manifest | Schema-validated, unknown fields refused, bounded sizes, origins without wildcards | — |
| T9 | Clickjacking the grant prompt | The prompt is editor chrome, outside the plugin frame; the frame cannot overlay it | — |
| T10 | Data residency | The disclosure names the endpoint; hosts route through their own backend; `?aiProvider=off` | The person's choice of BYOK endpoint is theirs |

## 11. The competitive survey

Pending on this branch: the cited survey of Google Docs, Word, ONLYOFFICE, open-source
editors, "AI-native" ONLYOFFICE forks, Notion and Coda (owner instruction, 2026-10-30) is
written here before any user-facing AI feature is built.

## 12. RAG, MCP both ways and the agent loop

Pending on this branch, with §11.

## 13. Proposed features and UI

Pending on this branch, with §11; the owner discusses it before P3.

## 14. Phases

| Phase | Delivers | Exit gate |
| --- | --- | --- |
| **P1** provider contract | `ai_provider*.mjs`: `checkRequest`/`checkResponse`, SSE, four adapters, resolution, budgets, disclosure; the host channel (`ai-provider` event, `ai-provider.answer`, `aiProvider` in process and on the element); BYOK settings; the three JSON Schemas | Every adapter against a local mock server (no real key), streaming and Stop; each guard driven red |
| **P2** plugin runtime | Manifest validation, the sandbox and CSP, the port RPC over `toolsFor`, permissions and the grant prompt, the Add-ins menu, side panel, context menu and palette contributions, `?plugins=` catalogues | A fixture plugin proposes an edit that reaches the private preview; a plugin without `read` is refused; a navigating plugin is disposed |
| **P3** assistant plugin | After the owner's review of §13 | — |
| **P4** further first-party plugins | After §13; licences checked first (no GPL, AGPL or non-commercial) | — |
| **P5** RAG, MCP client, agent loop | §12 | — |
