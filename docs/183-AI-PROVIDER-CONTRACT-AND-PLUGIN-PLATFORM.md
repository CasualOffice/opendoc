# 183 — The AI provider contract and the plugin platform

**Status:** Designed 2026-10-30. P1 (provider contract), P2 (plugin runtime), P3 (Casual AI) and P4 (LanguageTool, Readability)
built on `program/ai-plugins-2`; the owner chose F1–F7 on 2026-10-31 (§13) and set UI rules
that changed parts of §8 (§16). Proofing add-ins redone as writing issues in the text (§19), and
Casual AI corrected against Google Docs and Notion (§20), on `program/ai-main`.
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
   `{qid, chunk}` while streaming, then `{qid, response}` or `{qid, error, status?}`, told
   apart by which field is present (`kind` and `type` belong to the envelope); the event
   `{qid, kind: "cancel"}` withdraws a request. The same correlation-by-`qid` pattern as
   `directory-query` (`175` §7.2), with a 120 s ceiling per request, reset by each chunk.
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

## 11. The competitive survey (2026-10-30)

Public help pages, release notes and admin documentation only; every fact carries its URL.
"Unverified" means no primary source was found. No AGPL code was read for this section.
The load-bearing facts (ONLYOFFICE 10.0's licence change, Gemini's private suggestions) were
re-read at the source on 2026-10-30.

### 11.1 Google Docs (Gemini)

- **Entry points:** the "Ask Gemini" side panel; a bottom prompt bar ("hover over the spark
  near the bottom"); a floating **Refine** bar on a selection (Match writing style, Enhance,
  Rephrase, Shorten, Elaborate, More formal, More casual); `@` to reference files and skills,
  and a Sources button. <https://support.google.com/docs/answer/14206696>,
  <https://workspaceupdates.googleblog.com/2026/04/new-gemini-capabilities-in-google-docs-help-you-go-from-blank-page-to-brilliance.html>.
  `@` also references a Gemini Notebook (September 2026):
  <https://workspaceupdates.googleblog.com/2026/09/ground-ai-prompts-in-google-docs-on-existing-sources-from-Gemini-Notebook.html>.
- **Features:** summarise a document or selection, write and rewrite, tone, "Help me create"
  (formatted first drafts from Drive, Gmail, Chat and the web), "Match doc format" (a source
  document as a template), persistent custom instructions
  (<https://workspaceupdates.googleblog.com/2026/05/set-custom-instructions-for-gemini-in-Google-Docs.html>),
  images and diagrams
  (<https://workspaceupdates.googleblog.com/2026/07/generate-and-edit-visuals-with-gemini-in-Google-Docs.html>),
  and comment workflows: summarise threads, find unresolved issues, draft grounded replies and
  "generate suggested edits for you to review, approve, and seamlessly apply"
  (<https://workspaceupdates.googleblog.com/2026/07/streamline-collaboration-in-google-docs-with-Gemini-powered-comment-workflows.html>).
- **Applying edits:** Insert, Retry, Preview, Copy, Accept all and Reject all; suggested edits
  are "only visible to you until you approve them" (April 2026 post above). Conversation
  history is lost on reload (support page above). Whether an approved edit lands as a native
  Suggesting revision: unverified.
- **Grounding:** Drive, Gmail and the web (web only when asked); a Sources button; inline
  citations with Notebook grounding; retrieval honours Workspace permissions
  (<https://knowledge.workspace.google.com/admin/generative-ai/generative-ai-in-google-workspace-privacy-hub>).
- **Admin and privacy:** per-service on/off, default on
  (<https://knowledge.workspace.google.com/admin/generative-ai/workspace-with-gemini/manage-access-to-gemini-features-in-workspace-services>);
  non-regionalised features can be switched off per data region
  (<https://knowledge.workspace.google.com/admin/compliance/set-up-advanced-settings-for-data-regions>);
  no training on customer data; retention 90 days to indefinite; users are told when an
  injection defence fires (privacy hub above). Gemini only; no bring-your-own model.
- **Gate:** paid Workspace tiers and consumer AI plans (April 2026 post).

### 11.2 Microsoft Word (Copilot)

- **Entry points:** a Copilot button opening the chat pane, "Edit with Copilot" on a
  selection and in the mini toolbar, an auto-summary at the top, a "Chat only" mode, and `/` to
  reference a file, mail or meeting
  (<https://support.microsoft.com/en-us/office/welcome-to-copilot-in-word-2135e85f-a467-463b-b2f0-c51a46d625d1>,
  <https://support.microsoft.com/en-us/word/edit-with-copilot-in-word>).
- **Features:** draft from a prompt, outline or referenced file; rewrite, tone, concise;
  summary, key points, open questions, actions; convert text to a table
  (<https://support.microsoft.com/en-us/word/copilot/draft-and-add-content-with-copilot-in-word>);
  "Edit with Copilot" (formerly Agent Mode) creates, reformats and reorganises; the
  Researcher agent writes cited reports over web and work content
  (<https://learn.microsoft.com/en-us/microsoft-365/copilot/researcher-agent>). Copilot Pages
  inside Word: unverified.
- **Applying edits:** Replace, Insert below, Regenerate; Keep or Discard; in a shared
  document "nothing is added to the document until you review and approve it". The support
  page says Copilot respects Track Changes when it is on and warns that an edit can delete a
  comment anchored in the paragraph. Message Center MC1384426 (June 2026, mirrored at
  <https://mc.merill.net/message/MC1384426>) announces word-level tracked changes, visible by
  default; it disagrees with the support page, so we treat word-level tracking as shipping, not
  universal.
- **Grounding:** answers show references with citations into the document
  (<https://support.microsoft.com/en-us/office/chat-with-copilot-about-your-word-document-4482c688-a495-4571-bfcd-4a9fc6608090>);
  Microsoft Graph content the user can view, permissions honoured
  (<https://learn.microsoft.com/en-us/copilot/microsoft-365/microsoft-365-copilot-privacy>).
  Whether a citation jumps to the text: unverified.
- **Admin and privacy** (privacy page above): EU Data Boundary (Anthropic models excluded);
  no training; prompts and responses retained and searchable through Purview; admins allow or
  block OpenAI and Anthropic subprocessors; injection (XPIA) classifiers; agents allow-listed.
  No bring-your-own model.
- **Gate:** a Copilot licence; Edit with Copilot on premium tiers.

### 11.3 ONLYOFFICE

- **Entry points:** an AI tab, AI Chat, and an inline panel on Ctrl/Cmd+/
  (<https://helpcenter.onlyoffice.com/ai/gettingstarted/quick-start-guide/ai-quick-start.aspx>);
  from 9.0.4 a floating "AI inline agent"
  (<https://onlyoffice.com/blog/2025/08/meet-the-new-ai-agent-for-onlyoffice>); in 10.0,
  actions on a tab or the context menu (<https://www.onlyoffice.com/blog/2026/10/onlyoffice-ai-tools>).
- **Features:** generate, rewrite, expand, summarise, translate, grammar, tone; in 10.0 a
  generated table of contents, "Quick AI actions" for one-off tasks, and "AI assistants" that
  "work continuously and respond to changes in your document as you edit", document editor
  only (<https://www.onlyoffice.com/blog/2026/10/onlyoffice-docs-10>); optional web search and
  MCP servers configured as JSON (quick-start guide above).
- **Applying edits:** results stream into the document; rewrites "with optional track
  changes"; explanations as comments; no approval step is described (August 2025 post).
- **Providers:** bring your own key per user (OpenAI, Anthropic, Gemini, Mistral, DeepSeek,
  Groq, xAI, Together, OpenRouter, custom; Ollama and LM Studio by URL). Ollama needs
  `OLLAMA_ORIGINS`, i.e. the browser calls the provider
  (<https://helpcenter.onlyoffice.com/docs/userguides/ai/ollama.aspx>). Commercial 10.0 adds an
  admin panel with central providers, per-task model mapping and a server `/ai-proxy`
  (<https://api.onlyoffice.com/docs/ai/guides/server-ai-providers>).
- **Licence gate, the fact that matters most here:** 10.0 says "we've removed built-in AI from
  the open-source versions"; AI tools must be added separately
  (<https://www.onlyoffice.com/blog/2026/10/onlyoffice-docs-10>). An Apache-2.0 editor with a
  complete AI layer and a host proxy contract is therefore a licence wedge (SKILL §1).

### 11.4 Open-source and embeddable editors

- **Collabora Online 26.04:** optional AI in Writer (suggestions and improvements), off by
  default, admin-chosen provider, external or self-hosted
  (<https://www.collaboraonline.com/blog/cool-26-04-release/>). Entry points and how edits
  apply: unverified.
- **Nextcloud Assistant / Text:** a per-paragraph AI button, summarise and Q&A, Context Chat
  over your own files, local or OpenAI-compatible backends
  (<https://docs.nextcloud.com/server/stable/admin_manual/ai/app_assistant.html>); Context
  Agent uses tool calls and an MCP server
  (<https://docs.nextcloud.com/server/stable/admin_manual/ai/app_context_agent.html>).
- **SuperDoc** (AGPL community licence, facts only): agent edits arrive as tracked changes
  with accept and reject; "the model never edits raw XML. It calls structured tools"; an MCP
  server (<https://docs.superdoc.dev/getting-started/ai-agents>,
  <https://docs.superdoc.dev/extensions/track-changes>).
- **Tiptap AI Toolkit** (paid): agent tools for insert, proofread, edit and comments; AI edits
  shown as suggestions to accept or reject; any model
  (<https://tiptap.dev/docs/content-ai/capabilities/ai-toolkit/overview>).
- **CKEditor AI** (commercial): chat panel with history, Quick Actions on a selection, AI
  Review (grammar, style, tone), AI Translate, each switchable; suggestions as track changes;
  a Context Library of prompts and reference files; role permissions
  (<https://ckeditor.com/docs/ckeditor5/latest/features/ai/ckeditor-ai-overview.html>).
- **BlockNote AI:** an agent edits and formats, with accept or reject per suggestion and a
  visible step log; any model, bring your own key (<https://www.blocknotejs.org/docs/features/ai>).

### 11.5 ONLYOFFICE forks marketed as "AI"

- **Euro-Office** (a 2026 fork by Nextcloud, IONOS and others; ONLYOFFICE disputes it,
  <https://alternativeto.net/news/2026/4/onlyoffice-ends-its-partnership-with-nextcloud-over-new-unauthorized-euro-office-fork>):
  no primary source shows AI in the editor itself; the AI in that stack is Nextcloud
  Assistant around it.
- **ONLYOFFICE DocSpace 3.7** (the vendor's own product): agent chat that saves DOCX/PDF/PPTX,
  seven providers plus custom, an admin kill switch
  (<https://www.onlyoffice.com/blog/2026/06/onlyoffice-docspace-3-7>).
- No fork marketed as "AI-native" or "native AI ready" was found that ships AI of its own.
  Recorded as searched, not found.

### 11.6 Notion and Coda, for the patterns

- **Notion** (<https://www.notion.com/help/notion-ai-faqs>): Space on an empty line, `/AI
  Block`, "Edit with AI" on a selection, an agent chat, `@`-mention a page for context;
  changes proposed with accept / discard / try again; Q&A over the workspace and connected
  apps "with citations"; database autofill; admin toggles for web search, a model allow-list
  and per-member credit limits; zero-retention providers by default
  (<https://www.notion.com/help/notion-ai-security-practices>).
- **Coda:** a chat side panel, AI columns that fill cells from row data, AI blocks; Coda Brain
  cites sources and honours permissions
  (<https://coda.io/blog/ai/enterprise-ai-more-than-answer-questions>). The help pages
  answered 403, so the panel details rest on search snippets: unverified.

### 11.7 Industry practice, cited

- **Prompt injection:** OWASP LLM01:2025 "separate and clearly denote untrusted content",
  least privilege, human approval for privileged operations, output validation, adversarial
  testing (<https://genai.owasp.org/llmrisk/llm01-prompt-injection/>). Microsoft's spotlighting
  (data-marking untrusted input), Prompt Shields, consent and deterministic exfiltration
  blocking (<https://www.microsoft.com/msrc/blog/2025/07/how-microsoft-defends-against-indirect-prompt-injection-attacks>).
  Google: injection classifiers, markdown sanitisation with suspicious-URL redaction, user
  confirmation and notification
  (<https://blog.google/security/google-workspaces-continuous-approach-to-mitigating-indirect-prompt-injections/>).
- **Agency:** OWASP LLM06:2025 Excessive Agency: limit functions, permissions and autonomy,
  and put the approval inside the high-impact operation
  (<https://genai.owasp.org/llmrisk/llm062025-excessive-agency/>).
- **Evaluation:** groundedness and tool-call evaluators
  (<https://learn.microsoft.com/en-us/azure/foundry/concepts/built-in-evaluators>); OWASP's
  RAG triad (context relevance, groundedness, answer relevance).

### 11.8 What users generally get

| Capability | Docs | Word | ONLYOFFICE 10 | Notion | CKEditor | Tiptap / BlockNote / SuperDoc |
| --- | --- | --- | --- | --- | --- | --- |
| Side-panel chat on the document | yes | yes | yes | yes | yes | SDK |
| Inline or bottom prompt | yes | yes | Ctrl+/ | Space | — | SDK |
| Selection actions (rewrite, shorten, tone) | yes | yes | yes | yes | yes | yes |
| `@` or `/` references to other files | `@` | `/` | desktop attach | `@` | Context Library | — |
| Draft from a prompt and sources | yes | yes | yes | yes | yes | yes |
| Summarise and Q&A | yes | yes | yes | yes | yes | yes |
| Comment workflows | yes | announced | explain in a comment | — | yes | Tiptap, SuperDoc |
| Edits reviewed before they apply | private suggestions | approve / tracked | optional tracking | proposed | tracked | tracked |
| Citations to sources | yes | yes | clickable references | yes | — | — |
| Agents, MCP | Drive actions | agents | MCP | agent | API | MCP (SuperDoc) |
| Bring your own model or local LLM | no | no | yes (commercial admin in 10.0) | no | on-prem | yes |
| Admin switch, per-feature | yes | partial | yes | partial | per feature | app-defined |
| Licence gate | paid tier | Copilot licence | **removed from open source** | Business+ | commercial | paid / copyleft / AGPL |

**Table stakes:** a chat panel grounded in the document; selection actions; draft from a
prompt; summarise and Q&A; review before apply with Replace / Insert / Retry / Discard and
undo; an admin switch and no training on customer data.

**Differentiators in 2026:** AI edits as word-level tracked changes and AI on comment
threads; citations that jump to the source passage; bring-your-own provider with a host
proxy, local models and air-gapped use (now commercial-only at ONLYOFFICE); agents over
structured tools and MCP with host-set allow-lists; continuous review assistants and
persistent instructions; visible injection defences.

**What we already have that they do not, all built:** edits compiled to the engine's closed
vocabulary and shown privately before anything changes (`181` §4.4), tracked by default for
every role (Word tracks only when the person had tracking on); MCP and WebMCP over the same
registry; the engine refusing at the choke point whatever the UI decided.

## 12. RAG, MCP both ways and the agent loop

### 12.1 Retrieval with anchors (RAG)

- **Chunks:** `178` §4.4's anchored chunker (heading plus following paragraphs to a ceiling,
  a table header with a run of rows, a comment thread with its anchor), each with a
  deterministic id (chunker version + ordered anchors + text hash), its `NodeId` anchors and its
  heading path. Re-indexing re-embeds only chunks whose id changed: an edit is O(chunks
  touched).
- **Embeddings over the same contract:** OpenAI-compatible `POST {baseUrl}/embeddings`
  (`{model, input: string[], encoding_format: "float"}` → `{data: [{index, embedding}],
  model, usage}`, <https://platform.openai.com/docs/api-reference/embeddings>), through the
  host channel as a request with `purpose: "embed"`, or BYOK. Anthropic has no embeddings
  endpoint; with an Anthropic-only provider, retrieval falls back to lexical (below).
- **Index:** in the browser, a flat Float32 cosine index in a worker (exact search; a 300-page
  document is ~1,500 chunks × 1,536 floats ≈ 9 MB, searched in milliseconds), persisted in
  IndexedDB keyed by document fingerprint and chunk id, never inside the document (ADR-030 I4).
  An HNSW index is not needed at document scale and is deferred until a host collection needs
  it. **Hybrid**: BM25 over the same chunks, fused by reciprocal rank fusion, so retrieval
  works with no embeddings at all and names and numbers are found exactly.
- **Host collections:** a host may answer `purpose: "retrieve"` requests (`{query, k}` →
  `[{id, title, text, uri}]`) for its own corpus (Drive-like grounding); results are data, cited
  by `uri`, never followed automatically.
- **Answers with citations:** the model is asked to cite chunk ids in brackets; the UI turns a
  cited id into a chip that scrolls to and selects the anchored paragraph (`NodeId`, so it
  survives edits elsewhere). A citation to an id that was not retrieved is dropped and counted
  (output verification).

### 12.2 MCP both ways

- **Server (built):** `opendoc-mcp` and WebMCP (`178`, `181`).
- **Client (new):** the assistant may use tools from MCP servers the host lists
  (`?mcp=<catalogue>`) or the person connects in standalone settings, over Streamable HTTP
  (<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports>), never stdio
  in a browser. Each server has its own grant: which tools are allowed, "ask every time" or
  "allow for this document", and whether tool results may contain document text sent out.
  Tool descriptions and results are untrusted (§12.4): shown to the person, labelled with the
  server's origin, and never able to call editor write tools except through `propose_edits`.
  OAuth per the MCP authorization spec for servers that need it; tokens on the device only.

### 12.3 The agent loop

- **Shape:** read → plan → propose → self-check → present. The model gets the read tools and
  `propose_edits` (the private preview) as Chat Completions function tools, plus any granted MCP
  client tools. It cannot accept, apply directly, save or change grants.
- **Budgets:** at most 12 tool rounds, 60 s wall clock per round and 5 minutes per task, the
  session token budget of §3.6, and a visible step list ("Reading section 3…", "Proposing 4
  changes") with Stop, which aborts the in-flight provider call and every pending tool.
- **Long documents:** outline first; map-reduce summaries by heading; retrieval to pick the
  sections a question needs; edits planned per section against a fixed `base_revision`, so a
  stale plan is refused rather than applied over newer typing.
- **Self-check:** before presenting, the loop re-reads each proposed paragraph and asks the
  model to verify the change matches the instruction and introduces no facts absent from the
  cited sources; a failed check is shown on the card, not hidden.
- **Human in the loop by default:** every edit reaches the document only through the card's
  Accept, as suggestions by "Casual AI". Direct application exists only where the host
  granted it, and still through the card.

### 12.4 Practices

| Practice | Design |
| --- | --- |
| Injection defence | Document, comment, retrieval and MCP text is sent inside data-marked blocks (`<document_text id=…>`) the system prompt declares as data; instructions found there are never followed; the only write tool proposes; links in model output are rendered inert unless they point into the document; a detected instruction in data is shown to the person ("This text contains instructions; they were ignored") |
| Grounding and verification | Cited chunk ids validated against what was retrieved; quotes in answers checked against the anchored text; edit plans validated by the engine compiler before the card opens |
| Streaming and Stop | Chunks render as they arrive; Stop aborts the fetch; partial text is kept or discarded by the person |
| Token and cost limits | §3.6 budgets; a per-request estimate shown before long tasks ("about 40,000 tokens") |
| Logging and audit | A local, per-page log of each call: purpose, endpoint origin, model, token usage, duration, result code; never prompt or document text (`178` §5.9's rule). A host receives the same record on its channel to keep its own audit |
| Admin controls | Host config: `aiProvider=off`, an allowed-features list (`aiFeatures=chat,rewrite,…`), allowed BYOK presets or endpoints (`aiEndpoints=`), budgets, and the provider itself (which fixes data residency). Standalone settings show the same switches |
| Evaluation | `webapp/tests/ai_evals/`: about 30 tasks (rewrite, summarise with citations, translate, review, an injection document, a stale-revision plan), each with an expected outcome checked structurally (edit kinds, anchors, citations resolvable, injected instruction not obeyed), run in CI against a deterministic mock provider; the same set runs against a real model locally with a key, reported, never in CI |

## 13. Proposed features and UI, ranked by user value (for the owner's review)

Ranked by how often a user reaches for it (table stakes first), then by what nobody else
offers under a permissive licence.

| # | Feature | Entry points | How the result applies |
| --- | --- | --- | --- |
| 1 | **Selection actions:** rewrite, shorten, expand, fix grammar, tone (formal, friendly, confident), translate, summarise | A ✦ button in the selection toolbar, the context menu ("Casual AI ▸"), the palette, Ctrl/Cmd+J | Private card: redline, Replace / Insert below / Retry / Discard; Replace lands as suggestions |
| 2 | **Ask / chat** about the document, with citations | Side panel (Review ▸ Add-ins ▸ Casual AI, or Ctrl/Cmd+Shift+J) | Answers with citation chips that jump to paragraphs; "Insert" sends an answer to the card |
| 3 | **Draft from a prompt** at the caret, or a new document from a prompt and optional files | An empty-paragraph hint ("Press Ctrl+J to draft"), the panel, the palette | Card with the drafted paragraphs; Insert as suggestions |
| 4 | **Summarise the document** (summary, key points, open questions, action items) | Panel quick chip; File ▸ Info | Panel answer with citations; optional "Insert at top" |
| 5 | **Review this document** (clarity, consistency, terminology, factual claims flagged) | Review ▸ Add-ins ▸ Review with AI | Comments and suggestions authored "Casual AI", previewed as one card, grouped by kind |
| 6 | **Comment helpers:** draft a reply, summarise a thread, resolve-with-edit | A ✦ in each comment's actions | Reply draft in the comment box (the person sends it); resolve-with-edit through the card |
| 7 | **Grammar and style** (LanguageTool plugin, host-configured server) | Review band, live underlines like spell check | Suggestions from rule hits, previewed; no model needed |
| 8 | **Diagrams from text** (Mermaid plugin) | Insert ▸ Add-ins ▸ Diagram | A picture with its source kept for re-editing |
| 9 | **Ask across files** (host collections, RAG) | `@` in the chat box | Citations to host URIs |
| 10 | **Tools from MCP servers** | Settings ▸ AI ▸ Connected tools | Per-server grant prompt; results shown as steps |

**Mockups.** The card is the one already built (`181` §4.4); the new surfaces are:

```
Selection toolbar                          Context menu
┌──────────────────────────────────┐       ┌─────────────────────────┐
│ B I U  Link  Comment  ✦ Casual AI ▾│      │ Cut   Copy   Paste       │
└──────────────────────────────────┘       │ ───────────────────────  │
          ┌──────────────────────┐         │ ✦ Casual AI            ▸ │──┐
          │ Rewrite              │         └─────────────────────────┘  │
          │ Shorten   Expand     │                 ┌─────────────────────┴┐
          │ Fix grammar          │                 │ Rewrite  Shorten ... │
          │ Tone            ▸    │                 └──────────────────────┘
          │ Translate       ▸    │
          │ Summarise            │
          │ ──────────────────── │
          │ Ask about this…      │
          │ Sends text to api.openai.com │
          └──────────────────────┘

Side panel (Ask / Chat)                       Review band
┌ Casual AI ───────────────── ✕ ┐            ┌ … Track changes │ Comments │ Add-ins ▾ ┐
│ Sends text to api.openai.com  │            │                               ┌─────────┴────────┐
│ ───────────────────────────── │            │                               │ ✦ Casual AI       │
│ You: What does §3 commit to?  │            │                               │ ✦ Review with AI  │
│ AI: Three outcomes [1] [2] …  │            │                               │ ✓ Check grammar   │
│     [1] → 3. Goals ¶2         │            │                               │ ◇ Diagram         │
│ ── Reading section 3 ✓        │            │                               │ ───────────────── │
│ ── Proposing 2 changes  ■Stop │            │                               │ Manage add-ins…   │
│ [Summarise] [Key points] [Review]          │                               └───────────────────┘
│ ┌───────────────────────────┐ │
│ │ Ask about this document…  │ │
│ └──────────────── @ ─ Send ─┘ │
└───────────────────────────────┘

No provider configured (every AI entry point)
┌──────────────────────────────────────────────────────────────┐
│ Casual AI needs a model. Nothing is sent until you set one.  │
│ [Set up AI provider…]   (embedded: "Your administrator has   │
│                          not enabled AI for this editor.")   │
└──────────────────────────────────────────────────────────────┘
```

**Questions for the owner.**

- **F1.** Is the ranking right, and is anything missing that users expect (image generation
  is deliberately absent: it needs a different provider class)?
- **F2.** The ✦ entry in the selection toolbar: Docs and Word both put AI on the selection;
  our selection toolbar is small. One ✦ button with a menu, as drawn?
- **F3.** Ctrl/Cmd+J for selection actions and Ctrl/Cmd+Shift+J for the panel (Notion's
  binding; Ctrl+/ is ONLYOFFICE's and is taken here by the palette)?
- **F4.** Chat history: keep per document on this device (Docs loses it on reload), or never?
- **F5.** Review pass output: comments only, or comments plus suggestions?
- **F6.** MCP sampling: ship the adapter given its deprecation (§3.7), or drop it?
- **F7.** The eval set in CI with the mock provider only (as proposed), never a real model?

**Owner decisions, 2026-10-31 (build it):** F1 the ranking as is, no image generation; F2 one ✦
item in the selection toolbar; F3 a chord for the actions and the panel (built as ⌘/, see §16);
F4 chat history per document, on this device only, with Clear; F5 the review pass writes
comments plus tracked suggestions; F6 MCP sampling stays a mapping only, marked deprecated;
F7 evals in CI against the mock provider only.

## 14. Phases

| Phase | Delivers | Exit gate |
| --- | --- | --- |
| **P1** provider contract | `ai_provider*.mjs`: `checkRequest`/`checkResponse`, SSE, four adapters, resolution, budgets, disclosure; the host channel (`ai-provider` event, `ai-provider.answer`, `aiProvider` in process and on the element); BYOK settings; the three JSON Schemas | Every adapter against a local mock server (no real key), streaming and Stop; each guard driven red |
| **P2** plugin runtime | Manifest validation, the sandbox and CSP, the port RPC over `toolsFor`, permissions and the grant prompt, the Add-ins menu, side panel, context menu and palette contributions, `?plugins=` catalogues | A fixture plugin proposes an edit that reaches the private preview; a plugin without `read` is refused; a navigating plugin is disposed |
| **P3** assistant plugin | After the owner's review of §13 | — |
| **P4** further first-party plugins | After §13; licences checked first (no GPL, AGPL or non-commercial) | — |
| **P5** RAG, MCP client, agent loop | §12 | — |

## 15. Built (2026-10-30, `program/ai-plugins`)

### 15.1 P1: the provider contract

- `webapp/src/ai_provider.mjs` — the contract: `checkRequest` (field allowlist, message
  shapes, `max_completion_tokens` always set and clamped, the `bytes/4` input estimate refused
  locally as `context-too-long`, a 4 MiB body bound), `checkResponse` / `checkChunk` (only the
  contract's fields survive), `createAssembler` (chunks into one response, tool-call fragments
  merged by index), the error normalisation of §3.3, `createBudget`, `endpointAllowed`.
- `webapp/src/ai_provider_adapters.mjs` — `openAiAdapter` (bearer, Azure `api-key` and
  `api-version`, the `max_tokens` compatibility switch), `anthropicAdapter` (the three headers,
  system lifting, role merging, tool use, SSE event mapping), `hostAdapter`,
  `mcpSamplingAdapter`; one SSE parser; a watchdog for first byte (30 s) and idle (60 s);
  Stop keeps the text that had arrived as `partial`.
- `webapp/src/ai_provider_channel.mjs` — `createHostProviderChannel` (the `postMessage` half:
  `ai-provider` out, `ai-provider.answer` back by `qid`, a cancel on Stop and on timeout) and
  `createAiProviders` (the resolution of §3.8, budgets, an audit record per call with purpose,
  kind, endpoint, model, usage, duration and code, and never text).
- The host contract gained one event (`ai-provider`) and one verb (`ai-provider.answer`);
  `casual-doc-sdk`'s `HostEvent::AiProvider` and `HostRequest::AiProviderAnswer` keep
  `host_parity.rs` exact. `window.opendoc.aiProvider` in process, `client.serveAiProvider(fn)`
  on the host client, and the element's `aiProvider` property and `ai-provider="off"`.
- `webapp/src/ai_settings.mjs` and `ai_platform.mjs` — Settings ▸ AI provider and the palette's
  "AI provider settings…" (`ai.providerSettings`, an exact contract row requiring nothing): ten
  presets, Test connection through the same contract with a fixed prompt, Save, Forget, the
  disclosure, the key never written back into the form. Present and disabled with its reason
  in a frame or under `?aiProvider=off`.
- Schemas: `webapp/schemas/ai-provider-request.schema.json`, `-response`, `-chunk`.
- Started from `createAssistant`, so `main.js` carries no line.

### 15.2 P2: the plugin runtime

- `webapp/src/plugin_manifest.mjs` — `checkManifest` (§4, unknown fields refused),
  `effectiveGrant` (§6.3), `addedPermissions`, `sandboxCsp` and `sandboxDocument` (§7),
  `SANDBOX_TOKENS = ["allow-scripts"]`.
- `webapp/src/plugin_rpc.mjs` — JSON-RPC 2.0 over a `MessagePort` with the §5 bounds.
- `webapp/src/plugin_host.mjs` — one running plugin: the frame, the handshake, and every
  method of §5. `tools/list` and `tools/call` are `ai_webmcp.toolsFor` and `editor.ai`; edits
  only through `ai.preview`. A second `load` closes the plugin.
- `webapp/src/plugin_catalogue.mjs` — host, first-party and installed catalogues, grants per id.
- `webapp/src/plugin_platform.mjs` — the grant prompt, Manage add-ins, the Add-ins menu (Review
  ▸ Proofing), the side panel, the `plugin.` command family (palette and context menu), and
  the editor's `change`/`selection` events forwarded within each grant.
- `webapp/plugins/sdk/opendoc-plugin.js` — the client a plugin page includes;
  `webapp/plugins/catalogue.json` — the first-party catalogue, empty until P3/P4;
  `webapp/schemas/plugin-manifest.schema.json`; `tests/fixtures/plugins/probe/`.
- Host configuration: `?plugins=` / the element's `plugins` attribute; `docs/149`'s plugin rows
  and `docs/153`'s `shell.plugins` moved accordingly.

### 15.3 Guards, each driven red

| Guard | Mutation | Red |
| --- | --- | --- |
| A framed editor never uses BYOK | the `framed` check removed | "a framed editor never uses BYOK, even with a key stored" |
| Nothing provider-specific leaves | the field allowlist skipped | "checkRequest refuses a field outside the contract" |
| Stop reaches the host | no `cancel` event on abort | "host channel: … cancel on Stop" |
| The audit never carries text | the request added to the record | "the audit records … and never the text" |
| No provider, no network | a fetch made when unconfigured | "no provider configured: … NO network call" |
| Stop keeps what arrived | `partial` dropped | "openai: Stop mid-stream … keeps the text" |
| Anthropic from a browser | the direct-browser-access header dropped | the three Anthropic tests |
| The output ceiling holds | `max_completion_tokens` not clamped | "checkRequest always sets max_completion_tokens, clamped" |
| Scripts only | `allow-same-origin` added | "the frame may run scripts and nothing else" |
| No network without the permission | `connect-src *` | "the CSP connects nowhere unless network is held" |
| Unknown manifest fields refused | the field check skipped | "an unknown field is refused, not ignored" |
| Edits only propose | `tools/call` also calls `apply` | "an edit goes to the private preview and never to apply" |
| The mount caps the grant | the mount term dropped | "the effective grant is declared ∩ granted, capped by the mount" |
| A navigating frame is closed | the second-`load` check disabled | "a frame that navigates itself is closed and reported" |
| No file or session commands | the `NEVER` set skipped | "commands by id: no file, session, plugin or AI commands" |
| Tools need `read` | the `read` check dropped from `tools/list` | "without read a plugin may not list tools …" |
| The CSP comes first | the policy appended after the plugin's markup | "the CSP and the base come before the plugin's own markup" |
| No prototype methods | `handlers[method]` | "rpc: … unknown methods …" |

### 15.4 Not built, and why

- **MCP sampling has no transport in the editor:** the adapter is a mapping behind
  `useSampling(requester)`, marked deprecated with MCP 2026-07-28; nothing in the product
  supplies a requester (§3.7, owner F6: keep it as a mapping only).
- **ONLYOFFICE's `pluginsData` format is not read** (`docs/149`): a host lists OpenDoc
  manifests.
- **Residual T2** (a frame navigating itself can carry data out before it is closed) stands
  as §10 records it.

## 16. The owner's UI rules (2026-10-31), and what changed to meet them

The owner's rule: AI must not degrade the product's UI or UX. Nothing is visible until a
provider is configured; then at most three entry points, all in existing chrome; existing
tokens and components only; every output reviewable and undoable; streaming in the panel,
never in the document. Where the earlier design conflicted, the rule won:

| Was (P2, §8, §13) | Now | Why |
| --- | --- | --- |
| A floating plugin panel at the right edge | An ordinary `.side-panel` in the workarea (the outline's slot: same frame, head, close button, inset and width; a bottom sheet on a phone). One side panel at a time | "Docks in the existing side-panel slot"; `panel-alignment.spec.mjs` holds it like the others |
| The Add-ins button always on the Review band | Hidden until a running add-in contributes a ribbon row; Manage add-ins lives in Settings ▸ Add-ins | With nothing installed the chrome must be identical to today |
| A palette row "AI provider settings…" | Removed; Settings ▸ AI provider is its one home | "No menu rows" before a provider is configured |
| Context-menu rows for the assistant (§13 #1) | Not offered | Not one of the three entry points |
| ⌘J / ⌘⇧J (F3) | **⌘/** (Ctrl+/) | ⌘J is Justify, as in Word; ⌘⇧J and ⌘⌥J are Chrome's downloads and console. Ctrl+/ is ONLYOFFICE's own AI chord |
| Plugin pages styled by the plugin | `plugins/sdk/plugin.css` draws the editor's components from the editor's own tokens, sent at `initialize` and on every theme change; Inter and Material Symbols are the self-hosted faces | "Reuse, never invent"; both themes and a white-label `brand.css` follow |
| Plugin strings from the manifest | A first-party plugin's words come from the product's catalogue (`plugins.<name>.*`, 19 locales) through `ui/strings`; third-party plugins keep manifest `locales` | Localised like the rest of the chrome |

**Verified identical with AI off.** Screenshots of the editor and of its Review band at
1280×800, light and dark, are byte-identical to `origin/features/round3`'s (the comparison
swaps in round 3's `src/`, `editor.html` and locales on the same build).

## 17. Built: P3, Casual AI (RM-52)

A first-party plugin (`webapp/plugins/assistant/`) on the public API, listed in the first-party
catalogue with `when: "ai"`: it exists only while an AI provider is configured, and the
installation grants it `read`, `comment`, `suggest` and `ai` (no prompt for the product's own
add-in; the person can turn it off in Manage add-ins). It never holds `edit`: every change is
a suggestion.

- **Entry points (exactly three):** the ✦ item in the selection toolbar (Rewrite, Shorten,
  Expand, Fix grammar, More formal, Friendlier, More confident, Translate…, Ask), the Casual
  AI panel in the side-panel slot, and palette rows (one per command, plus "AI actions on the
  selection" on ⌘/).
- **The panel:** the disclosure ("Text you send goes to api.openai.com"), a conversation log
  (`role="log"`), streaming answers with a **Stop** button (Escape in the prompt too),
  Summarise / Key points / Action items / Review document / Reply to comments, an Ask or Draft
  composer, and Clear history. History is kept per document on this device only (F4), keyed by
  an opaque document key, at most 20 turns per document and 30 documents.
- **Every change** is a `propose_edits` plan: the private preview opens, nothing reaches the
  document until Accept, and Accept lands tracked suggestions by "Casual AI" in one undo step.
  Selection actions replace exactly the selection (`replace_text` anchored on the quote nearest
  the selection); across paragraphs, each touched paragraph is rewritten (`rewrite_block`). The
  review pass writes comments plus tracked suggestions (F5). Drafts insert paragraphs after the
  caret's paragraph. Replies land on their thread.
- **Answers cite** paragraphs as chips (¶2) that move the caret to the paragraph.
- **`assistant_core.js`** holds every prompt, the data marking, the BM25 context selection to a
  token budget, the output verification (only the action's own operations, only on paragraphs
  sent, only on quotes that occur; the rest dropped and counted) and the plan builders. The UI
  and the eval set use the same file.
- **Evals (F7):** `webapp/tests/ai_evals.test.mjs`, 31 tasks with expected outcomes, in the unit
  lane against a mock provider; includes an obeyed injection that still cannot leave the
  action's edits.
- **Platform additions it needed, all generic:** the `selection` contribution (the ✦ menu),
  `ui/reveal`, `ui/strings` (first-party only), design tokens and a document key at
  `initialize`, `theme/changed` with tokens, `document/opened`, `when: "ai"` and
  installation grants in the first-party catalogue.

**Guards, each driven red:**

| Guard | Mutation | Red |
| --- | --- | --- |
| No AI chrome without a provider (e2e) | the `when: "ai"` gate removed | "with no AI provider the editor shows nothing of Casual AI" |
| Stop stops (e2e) | the Stop handler does nothing | "Stop ends a stream at once and says so" |
| Changes wait in the preview (e2e) | `propose` replaced by a command | "✦ Rewrite streams in the panel, proposes in the preview…" |
| A closing tag cannot end a data block | the neutralisation removed | "a closing tag inside the document cannot end its data block" |
| Review quotes are verified | the quote check removed | "the review writes comments and tracked suggestions on real quotes only" |
| Citations are verified | unsent ids kept | "a citation to a paragraph that was not sent is dropped and counted" |
| Multi-paragraph edits stay inside the selection | unknown ids accepted | "a multi-paragraph rewrite touches only the paragraphs sent" |
| Document text never in the system prompt | context appended to the system prompt | "document text never enters the system prompt" |
| Context respects its budget | the budget check disabled | "a long document is cut to the budget by relevance…" |

**Not built in P3, and why.** Image generation (F1: excluded). Ask across files and MCP tools
(§13 #9, #10) are RM-54 with RAG and the MCP client. Drafting a new document from a prompt
(§13 #3, second half) waits for a "new document from text" path in the editor.

## 18. Built: P4, two more first-party add-ins (RM-53)

**Licences checked first.** LanguageTool's server and libraries are LGPL-2.1
(<https://github.com/languagetool-org/languagetool>); the add-in contains none of that code. It
talks to a server over the documented HTTP API (`POST /v2/check`,
<https://languagetool.org/http-api/>), as any client does. Mermaid
(<https://github.com/mermaid-js/mermaid>) is MIT, so its licence is not the obstacle; the
obstacle is that the webapp has no runtime npm dependency at all (`docs/182` records PDF.js
being refused as "the first runtime npm dependency"), and Mermaid with its rendering
dependencies would be several megabytes of vendored code, and a diagram can only be inserted as
a picture, which the engine does not track as a suggestion (`insert_image` is not trackable, so
an add-in could insert one only under a host's direct grant). **Mermaid is therefore not built;
the owner's call** (vendoring a library and an untracked insert path). In its place:
**Readability**, which needs no third-party code and fits the suggestion model.

- **LanguageTool** (`webapp/plugins/languagetool/`): `read`, `suggest`, `network` to
  `https://api.languagetool.org`. Off until turned on; it is the one first-party add-in with no
  installation grant, so turning it on shows the grant prompt naming the server text goes to. An
  administrator points it at their own server in the host catalogue (`network` and
  `config.server`, `docs/183` §9; a person's own install cannot). The panel checks the document in
  chunks of whole paragraphs, lists each issue with the flagged text and up to three
  replacements, proposes one fix or all of them as tracked suggestions, and drops a match that
  spans paragraphs or no longer matches the text (counted, never guessed).
- **Readability** (`webapp/plugins/readability/`): `read`, `comment`; installed with the product
  and runs entirely in its sandbox (no network permission, so the CSP lets it reach nothing).
  The Flesch reading ease and Flesch-Kincaid grade for English, long sentences (over 25 words) in
  any language written with spaces, and passive voice and filler words in English, each proposed
  as a comment. The heuristics are stated in the code and the panel says when a score is
  English-only.
- **Signed as themselves.** An add-in's proposal is authored "<name> (add-in)", not "Casual AI":
  a new engine export `aiPreviewFrom` (`crates/casual-doc-wasm/src/ai.rs`) takes the add-in's
  manifest name from the editor and appends " (add-in)" itself, so no add-in can sign as a
  person; `aiPreview` and the assistant are unchanged.
- **Shared panel layout** in `webapp/plugins/sdk/panel.css`, tokens only.

**Guards, each driven red:** the add-in author (`an_add_in_signs_as_itself_and_never_as_a_person`:
the suffix dropped), chunk boundaries, the cross-paragraph drop, the overlap rule in fix-all, the
English-only switch, the published formula, comments-only plans
(`webapp/tests/plugin_firstparty.test.mjs`), and in a browser the grant prompt naming the server,
the signed preview, and Readability making no request off the page
(`webapp/tests/e2e/ai-addins-firstparty.spec.mjs`).

**Superseded in part by §19 (2026-10-31):** the panel lists described above are gone. Both
add-ins now report writing issues, and the editor draws them in the text.

## 19. Writing issues in the text: the owner's rejection, the survey, and the design (RM-53)

On seeing the §18 screenshots the owner rejected the design: "Is this UX industry-proven?
Have you ever seen how LanguageTool behaves?" §18 listed issues in a side panel, and no
product people know works that way. This section records what those products actually do,
then the design that replaced the panel.

### 19.1 What the products do (cited; surveyed 2026-10-31)

Every claim cites the vendor's own page. Where a vendor documents nothing, the table says
so rather than guessing.

| | LanguageTool add-on | Grammarly | Google Docs spelling and grammar | Hemingway |
| --- | --- | --- | --- | --- |
| Where issues show | Underlined in the field as you write: style, spelling and grammar ([add-on](https://languagetool.org/insights/post/add-on)) | Underlined in the field ([extension guide](https://support.grammarly.com/hc/en-us/articles/115000091592-Grammarly-s-browser-extension-user-guide)) | Underlined in the document ([57859](https://support.google.com/docs/answer/57859)) | Highlighted in the text ([highlighted issues](https://hemingwayapp.com/help/docs/highlighted-issues)) |
| Colour by category | Red for spelling, yellow for grammar, blue for style ([LT Insights](https://languagetool.org/insights/?p=98994)) | Red for correctness, blue for clarity, green for engagement, purple for delivery ([Grammarly blog](https://www.grammarly.com/blog/product/better-writing-with-grammarly/)) | Red for spelling, blue for grammar ([57859](https://support.google.com/docs/answer/57859)) | Yellow and red for hard-to-read sentences, blue for weakeners (adverbs, passive voice, qualifiers), purple for simpler words ([highlighted issues](https://hemingwayapp.com/help/docs/highlighted-issues)) |
| On the word | A card with Ignore and "Turn off rule everywhere" ([rules](https://languagetool.org/insights/post/enabling-and-disabling-rules)) | Hovering shows the suggestion and a click accepts it; the card has Dismiss and a book icon that adds the word to the dictionary ([guide](https://support.grammarly.com/hc/en-us/articles/115000091592-Grammarly-s-browser-extension-user-guide)) | Clicking the word shows the fix, with Add to dictionary and Ignore ([14701081](https://support.google.com/docs/answer/14701081)); right-clicking offers the same ([57859](https://support.google.com/docs/answer/57859)) | Clicking opens a popup with "Use suggestion", or the synonym to apply ([highlighted issues](https://hemingwayapp.com/help/docs/highlighted-issues)) |
| Count | A circled red number opens an overview ([add-on](https://languagetool.org/insights/post/add-on)) | The corner icon spins while checking, then shows the number of suggestions ([guide](https://support.grammarly.com/hc/en-us/articles/115000091592-Grammarly-s-browser-extension-user-guide)) | Not documented | A grade in the sidebar ([readability](https://hemingwayapp.com/help/docs/readability)) |
| Keyboard | Not documented | Not documented (only "Edit shortcuts") | Next misspelling ⌘' / Ctrl+', previous ⌘; / Ctrl+; ([179738](https://support.google.com/docs/answer/179738)) | Not documented |
| List or panel | Only an overview, opened from the count | An optional sidebar, opened from the count ([Docs sidebar](https://www.grammarly.com/blog/product/grammarly-google-docs-sidebar/)) | A separate "Spelling and grammar check" tool, Ctrl+Alt+X ([57859](https://support.google.com/docs/answer/57859)) | A sidebar of counts |

**What all of them share:** the issue is shown where it is, and fixed where it is. A list is
at most optional, opened from a count, and never the default.

**Not documented anywhere we found, so not claimed:**

- how long LanguageTool waits after typing before it checks;
- whether Google Docs' card has a tick and an X;
- what accepting a Docs suggestion does in Suggesting mode.

### 19.2 The design

- **The add-in supplies matches; the host draws them.** A manifest declares
  `contributes.proofing: {}`, which requires `read`.
  - The host sends `proofing/check` with the paragraphs that changed:
    `[{node, text, language}]`.
  - The add-in answers with `proofing/report`:
    `{results: [{node, text, matches: [{start, end, category, message, replacements, rule}]}], summary}`.
  - Offsets are UTF-16, into the text the add-in was sent.
- **It reuses the built-in spell check rather than a second mechanism.** `spell_check.mjs`
  gained one hook, `setExternal(layer)`. Through it, add-in marks get the same:
  - window scan: only the pages in view are checked, never the whole document;
  - debounce: 400 ms after an edit, 90 ms after a scroll;
  - `decorationRectsOnPages` batch: one engine call for every mark on screen;
  - overlay classes: `.spell-error` or `.grammar-error` for an underline.

  With no proofreading add-in running the hook is null, and the checker behaves exactly as
  it did before.
- **Only changed paragraphs are checked again.** The host remembers what it sent each
  add-in. Answers are cached by text, so an undo brings its marks back at once without a new
  request (`addin_proofing.mjs`).
- **Answers are verified, not believed.** The host drops:
  - a result about text the paragraph no longer holds;
  - a match whose quoted text is not what sits at its offsets;
  - a match with an unknown category;
  - a match outside the paragraph.

  `LIMITS` bounds every report, paragraph, message and replacement.
- **Colour encodes the category, and every colour is an existing paper token.** No new
  colour exists. Tints have no border, so they never read as a comment range.

  | Category | Drawn as | Token |
  | --- | --- | --- |
  | Spelling | Squiggle | `--paper-spelling` (red, as the built-in squiggle) |
  | Grammar and punctuation | Squiggle | `--paper-grammar` (blue, as the built-in squiggle and Docs) |
  | Style | Squiggle | `--paper-comment` (amber) |
  | Readability: long sentences | Hemingway-style tint, no border | `--paper-comment` |
  | Readability: passive voice | Tint | `--paper-insert` |
  | Readability: filler words | Tint | `--paper-grammar` |
- **One underline per word.** When an add-in underlines a span, the built-in squiggle is
  withheld there (`covers`), and so is the built-in context-menu row.
- **The card** is the existing inline card at the word (`.review-inline-card`, the one
  tracked changes use). It shows:
  - the category, with its colour dot;
  - the add-in's name and the message;
  - up to three replacement chips;
  - Ignore;
  - for spelling, Add to dictionary. This writes to the editor's own personal dictionary,
    the one the built-in checker uses.

  A click that leaves the caret in a mark opens the card. With no add-in running, a click
  does what it did before.
- **Applying a fix** goes through the built-in spell check's path: `replaceMisspelling`, which
  is `doc.replaceRanges` inside `runEdit`.
  - It is one undo step.
  - In Suggesting mode it lands as a tracked suggestion; in Editing mode it is a direct
    edit.
  - In Viewing mode the chips are disabled, with the reason.

  The mark goes at once. After a tracked replacement the deleted word is still in the
  paragraph; it is held back on that text and not underlined again. An undo back to the old
  text brings the mark back.
- **Keyboard and screen reader.** These commands (`plugin.proofing.*`) exist only while a
  proofreading add-in runs:
  - ⌘' / Ctrl+' and ⌘; / Ctrl+; go to the next and previous issue, Google Docs' own chords.
    Each one selects the word, opens its card with focus on the first fix, and announces
    "Issue n of m: category. message" in the review live region.
  - ⌥⏎ / Alt+Enter opens the card at the caret.
  - Escape closes the card and returns focus to the text.

  All three are palette rows too.
- **The count** is in the existing status bar, styled like the compatibility count
  (`#addinIssuesStatus`). It is hidden unless a proofreading add-in runs. A click goes to the
  next issue. Its tooltip names the add-ins and gives Readability's reading-ease score.
- **No panel.** Both add-ins dropped their panel, ribbon row and commands, and their pages
  have no interface at all.
- **Off by default.** The grant prompt is unchanged. LanguageTool still asks for `read`,
  `suggest` and `network` to api.languagetool.org, and names the server. Readability needs no
  prompt.

### 19.3 An add-in can never edit without the person's click

No RPC applies anything. `proofing/report` only describes issues, and
`tests/addin_proofing.test.mjs` asserts that no plugin method matches apply, replace or
accept. The host applies a replacement only when all four of these hold:

1. **The person acted in the host's own card.** The click or key press reached the card, and
   `apply` returns unless `event.isTrusted`, so a synthetic event cannot apply anything.
2. **The replacement is one the host holds.** It is one of the verified replacement strings
   the host keeps for that mark.
3. **The text is still there.** The host re-reads the span from the document at that moment
   (`copyText`). If the text has changed, nothing is replaced and the person is told.
4. **The add-in holds `suggest` (or `edit`).** Without it, the card shows the message and
   Ignore but no chips. That is why Readability, which holds `read` and `comment`, offers no
   fix.

This is the narrow, host-mediated permission the owner asked for: "apply a text replacement
the person clicked". It adds no new permission word. `suggest` already means "may propose
changes the person reviews", and a chip in the host's card is that review. The edit is then
the person's own, exactly as when they accept a built-in spell-check correction.

### 19.4 Not built, and why

- **"Turn off this rule everywhere"** (LanguageTool's card). Ignore covers one occurrence; a
  per-rule switch needs a place to manage rules, and none exists yet.
- **An opt-in issue list.** The brief allowed one from the palette. Next, Previous and the
  count already reach every issue, so none was needed. If one is ever wanted it is a palette
  row, as the brief said.
- **Checking pages outside the view.** The check is windowed, like the built-in checker. The
  count covers the paragraphs that have been in view, as Word's own checker does while it
  scans.

## 20. Casual AI compared with Google Docs Help me write, the Gemini panel and Notion AI

Compared on 2026-10-31 against the vendors' own pages: Google's
[13951448](https://support.google.com/docs/answer/13951448) and
[14206696](https://support.google.com/docs/answer/14206696), the
[April 2026 Workspace update](https://workspaceupdates.googleblog.com/2026/04/new-gemini-capabilities-in-google-docs-help-you-go-from-blank-page-to-brilliance.html),
and Notion's [notion-ai-faqs](https://www.notion.com/help/notion-ai-faqs) and
[notion-ai-for-docs](https://www.notion.com/help/guides/notion-ai-for-docs).

| Deviation | Established practice | Now |
| --- | --- | --- |
| A ✦ action on a selection opened the side panel and streamed into its log | Docs' Refine on the floating bar and Notion's Edit with AI work at the selection. In Docs the result is a suggestion only the person sees until they approve it | **Fixed.** A selection action leaves the panel closed. Progress goes to the status line ("Casual AI: Rewrite…") and the result opens directly in the private preview, our counterpart of Docs' approve step. Ask and Translate still open the panel because they need input there (`plugin_platform.mjs` `invoke`) |
| An Ask answer had no actions | Gemini's side panel offers Insert, Retry, Copy and thumbs on each response | **Fixed.** Every answer now has Insert and Try again; Insert goes through the preview as a suggestion. Copy is not offered: the sandbox holds no clipboard permission, and the text can be selected. Thumbs are not offered: there is no feedback channel, and nothing is sent anywhere |
| The disclosure named the endpoint but not that the model can be wrong | Gemini warns that it is still learning and may not support a request | **Fixed.** The disclosure adds "AI can make mistakes: check what it writes before you accept it." |
| — | Gemini summarises the document automatically when its panel opens | **Deliberately not copied.** It sends the whole document to a model without a request, against owner decision 1 ("only on a person's request"). Summarise is one chip away |
| — | Notion opens the prompt on Space on an empty line; Docs shows a margin button | **Not copied.** Space must type a space, and a margin button would be new chrome. ⌘/ with no selection opens the panel's prompt, and the palette has every command |
| — | Docs' Refine offers Bulletize and Summarize on a selection | **Not yet built.** A bulleted answer has to become list paragraphs, which needs the multi-paragraph `rewrite_block` path for a single selection. Recorded under RM-52 |
| Stopping a selection action | In Notion, Escape discards while it writes | **Partly met.** Stop and Escape work in the panel. A selection action running while the panel is closed is stopped by opening it (⌘/ with no selection) |

Kept as they were, because they already match established practice:

- the private preview before anything reaches the document, which is Docs' "visible only to
  you until you approve";
- tracked suggestions signed "Casual AI";
- citation chips that move the caret to the paragraph;
- Clear history;
- Enter to send and Shift+Enter for a new line.

## 21. The Casual AI panel, redesigned after the owner's review (RM-52, 2026-10-31)

The owner on the §20 build: "too crowded, redundant information, too narrow, we need something
tangible that basically works; it would be nice to have it on the right side, full page height,
and drag to expand the width." Measured against Gemini's side panel in Google Docs and Copilot's
pane in Word, the panel had four problems: it sat in the 252px outline slot on the LEFT; it showed
the endpoint, the caveat and a history note on every open; it had two rows of actions (five chips
and an Ask/Draft select) above the composer; and an answer's only route into the document was a
quiet "Insert" link.

**Placement (`editor.html`, `style.css`, `plugin_platform.mjs`).** `#pluginPanel` is now the last
item of `.workarea`'s row: flush to the window's right edge, from right below the ribbon to the
bottom of the window. It reaches down past the status bar, whose own right edge moves in by the same
width (`--plugin-panel-foot` is the status bar's measured height, 0 when it is hidden). The page
column is the flex item that shrinks, so nothing covers a page. It keeps the `.side-panel` shell
(head, title, close button, inset; `panel-alignment.spec.mjs` still holds it) but drops the
floating card's inset, radius and shadow for one hairline on its inner edge: a docked pane, as
Docs' and Word's are. What is pinned to the window's right edge moves in beside it: Find and
replace, and the private preview, which docks at the foot of the panel over its composer while
the panel is open. On a phone it is the bottom sheet every side panel becomes. It is the slot for
every add-in panel, not only Casual AI's (Office add-in task panes are on the right too).

**Resizable (`src/plugin_panel_width.mjs`).** The WAI-ARIA window splitter on its inner edge:
`role="separator"`, focusable, `aria-valuenow`/`-min`/`-max`; a pointer drag; ← / → by 16px
(Shift: 64px), the arrow pointing away from the panel widens it (mirrored right to left);
Home/End for the two ends; a double-click resets it (VS Code's sash). At least 320px, at most half
the window, 400px by default; remembered on this device under `opendoc.pluginPanel.width`
(try/catch, so a refusing storage only means the default); re-clamped when the window changes,
keeping the remembered width so the panel grows back with the window.

**Decluttered.** One header line, drawn by the editor: the title, ⋯ and Close. ⋯ holds the
add-in's own panel commands (a new manifest field, `contributes.panel.menu`, refs to its declared
commands; Casual AI's is Clear history) and, for an add-in holding `ai`, the HOST's disclosure
sentence (`disclosureKey`, so an add-in cannot word it) and "AI provider settings…". The body is the
conversation. While it is empty it shows four suggestions (Summarise, Key points, Draft…, Review
document), gone at the first message, and, once per device, where text goes and that history stays
on the device. ONE composer: a growing text field with Send that becomes Stop, a chip naming the
selection only while there is one (× asks without it), and a "Draft at the cursor" chip while
drafting. One line under it: "AI can make mistakes". Gone: the permanent endpoint line, the
Ask/Draft select, the Action items and Reply to comments chips (Reply to comments is a palette
command), the history note and the Clear history button.

**Tangible answers.** Every answer is a card whose first button changes the document:

| Answer | Primary | Also |
| --- | --- | --- |
| A question, a summary, key points | Insert at the cursor | Add as comment |
| A question with the selection attached, answered as a revision of it | Replace selection, with an inline diff in the card | Add as comment |
| A question with the selection attached, answered about it | Add as comment (on the selection) | Replace selection |
| A draft | Insert at the cursor | — |
| A review | Apply as suggestions, with each finding's diff and comment in the card | — |
| A ✦ action on the selection | Replace selection (proposed at once, as §20 decided; the button re-proposes after a discard) | — |

Plus Copy (a new first-party-only host method, `ui/copy`, 64 KiB; the sandbox holds no clipboard)
and Try again (re-runs the turn in place of the card). "Revision or answer" is
`looksLikeRevision`: one paragraph no longer than twice the selection plus 40 characters, or
keeping 60% of its words. Insert at the cursor is new: one paragraph goes INTO the text at the
caret (`insert_text` at the caret's byte offset), several become paragraphs after it, and a list
answer becomes one paragraph per item. Every action still goes through `propose_edits`, the private
preview and Accept, as tracked suggestions by "Casual AI" in one undo step: `assistant_core.js`
builds each plan from the action's own operations only (`planForInsert`: `insert_text` or
`insert_paragraphs`; `planForComment`: `comment`; the rest as before). The inline diff is
`wordDiff`, a word-level LCS bounded at 160,000 token pairs. Drafts and summaries join the
conversation history, so "write me a paragraph based on our discussion" has the discussion.

**Guards, each driven red** (the mutation and the red line are in the commit message):
`tests/ai_panel.test.mjs` (clamp, keys, storage, the menu field, the plans, the diff, the
selection rule) and `tests/e2e/ai-panel.spec.mjs` (full height on the right with no page under it at
1280×800 and 1920×1080; drag, keyboard, clamping and persistence; each card action landing in the
document; the discussion draft; Copy).

## 22. Further owner requirements of 2026-10-31: designed, not built in this lane

Four requirements arrived while §21 was being built. Each is larger than a panel change: each
touches the engine's tracking rules, the edit path or the collaboration layer, and the owner's
standing rule is that such work ships as ONE complete lane, not as a partial merge. They are
recorded here with what was measured, so the lanes start from facts.

1. **Tables from AI ("Turn into table", "Describe this table").** Measured: `insert_table` and
   `text_to_table` are untracked commands (`casual-doc-command` `registry.rs`,
   `TABLE_NOT_TRACKABLE`), and `casual-doc-ai`'s planner refuses them in suggest mode
   (`refuse_untracked`, `plan.rs`). `insert_table` builds an EMPTY table and `set_cell_text` needs
   the new table's id, which a plan cannot name before it exists. The one existing path that builds
   a filled table in one undo step is `pasteExternalStructured` (the clipboard's real-table paste),
   which also refuses Suggesting mode. So a real table from Casual AI is a DIRECT edit, which the
   assistant does not hold (it is granted `suggest`, never `edit`). The design: a first-party-only
   host method that takes validated rows (rectangular, at most 50 × 20, a header row), previews
   them in the card, and on the person's click inserts through `pasteExternalStructured` at the
   end of the source paragraph, saying in the card that a table is added directly because the
   engine cannot track a table insertion. Refused in Suggesting mode and without the person's
   edit capability. "Describe this table" needs `read_blocks` to return a table's cells as
   structured data and an `insert_paragraphs` anchor after a table.
2. **Edits streamed into the document by a visible co-editor** (a "Casual AI" caret and presence
   flag from `collab_presence.mjs`, a Figma-style pointer with a name tag on the overlay layer,
   gliding with eased motion and jumping under `prefers-reduced-motion`; tokens applied every
   50–100 ms as tracked insertions; Stop keeps them; Accept all / Reject all per turn; one undo
   step per turn; incremental validation that rolls the turn back on a violation; the user typing
   elsewhere is preserved). This replaces the private preview for edit actions, which ADR-077 and
   §16 made the safety model, and needs: turn-scoped history coalescing in the engine, an apply
   path for a first-party add-in that is not `propose`, a presence participant that is not a room
   member, and position transforms of the AI's anchor against local edits (ADR-030 I3). It is a
   lane of its own, carried end to end.
3. **"AI edits as: Suggestions | Direct edits"**, capped by the document's mode, protection and
   the person's role, told to the model in a capability line and enforced by the validator, with an
   access badge in the composer and the e2e role matrix. Depends on 2 for "Direct edits stream in".
4. Everything above keeps §21's panel.
