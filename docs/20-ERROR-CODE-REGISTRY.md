# Error Code Registry

**Status:** Accepted for Phase 0
**Last updated:** 2026-07-24
**Tracker:** F-006

## Contract

Every error crossing the SDK, WASM, C ABI, or serialized operation boundary has:

- a stable `ODC-NNNN` code;
- a machine-readable category;
- a severity;
- a safe human-readable message;
- optional structured context;
- an optional internal source chain that is excluded from untrusted output.

Error codes are never recycled. Message wording may improve without a breaking
release, but code meaning may not.

## Severity

| Severity | Meaning |
| --- | --- |
| `warning` | Operation can continue with a documented limitation. |
| `error` | Requested operation failed; session remains valid. |
| `fatal` | Session cannot safely continue. |

Cancellation is an expected non-fatal error, not a warning or panic.

**Severity is a property of the code; terminality is a property of the message carrying
it.** The `ODC-7xxx` collaboration family makes that visible: the same code can arrive in a
message that ends the session or in one the connection survives, and
`casual_doc_transaction::protocol::Refusal` answers `is_terminal` and `is_retryable`
separately for exactly that reason. Collapsing the two is how a client comes to retry
something it must never send again. See `152` §5.6.

## Initial Registry

| Code | Name | Severity | Meaning |
| --- | --- | --- | --- |
| `ODC-0001` | `invalid_argument` | error | A public argument is malformed or inconsistent. |
| `ODC-0002` | `invalid_configuration` | error | Engine or session configuration is invalid. |
| `ODC-0003` | `unsupported` | error | The requested operation is not implemented or allowed in the active profile. |
| `ODC-0004` | `cancelled` | error | A cancellable operation was stopped without corrupting session state. |
| `ODC-1001` | `malformed_document` | error | Input cannot be represented as a valid normalized document. |
| `ODC-1002` | `unsupported_content` | warning | Content is not fully supported but may be preserved or flattened. |
| `ODC-1003` | `resource_limit` | error | A configured parser or runtime resource limit was exceeded. |
| `ODC-1004` | `policy_denied` | error | Host or runtime security policy denied the action. |
| `ODC-1005` | `external_resource_denied` | warning | An external relationship was not fetched under the default policy. |
| `ODC-2001` | `stale_revision` | error | A transaction base revision does not match the session revision. |
| `ODC-2002` | `invalid_position` | error | A position does not resolve to a valid boundary. |
| `ODC-2003` | `empty_transaction` | error | A transaction contains no effective operations. |
| `ODC-2004` | `invalid_text_input` | error | Text contains a control requiring a different structural command. |
| `ODC-2005` | `invariant_violation` | fatal | Committed or imported model state violates a required invariant. |
| `ODC-2006` | `history_empty` | error | The requested undo or redo stack has no entry. |
| `ODC-3001` | `resource_unavailable` | error | A required font, image, or host resource is unavailable. |
| `ODC-4001` | `layout_failed` | error | Layout could not complete for the requested content/configuration. |
| `ODC-5001` | `render_failed` | error | A renderer failed without invalidating document state. |
| `ODC-6001` | `import_failed` | error | Format import failed after input passed initial sniffing. |
| `ODC-6002` | `export_failed` | error | Format export failed; existing session state remains valid. |
| `ODC-7001` | `collaboration_conflict` | error | A remote operation cannot be safely applied or rebased. *That one action did not take; redo it.* |
| `ODC-7002` | `collaboration_protocol_version` | fatal | The two ends do not speak the same protocol version. Terminal, and **never retried** — a client that retries a version mismatch loops for ever. |
| `ODC-7003` | `collaboration_not_authorised` | fatal | The session is not authorised. Deliberately undetailed: detail is useful to an operator in a log and useful to an attacker in a response. |
| `ODC-7004` | `collaboration_read_only` | error | This participant may read but not write. Enforced at the operation, not by hiding a control. |
| `ODC-7005` | `collaboration_not_saving` | error | The ordered session cannot persist work. *Copy it out.* Distinct from `ODC-7001` because the two ask the user for opposite things. |
| `ODC-7006` | `collaboration_too_far_behind` | error | The participant is behind the retained history and cannot be caught up by replay. Work it had not had acknowledged is lost, and saying so is the point of the code. |
| `ODC-7007` | `collaboration_malformed` | error | The message could not be read. The sender must **not** send the same bytes again. |
| `ODC-7008` | `collaboration_id_collision` | error | An arriving operation introduces an identity this replica already holds, or one minted outside the sender's own identity space. Refused rather than applied, because applying it overwrites a node somebody else minted. |
| `ODC-7009` | `collaboration_stale_base` | error | The submission was written against an ordered position the document has moved past. Nothing is lost: the client rebases it and resubmits with the same sequence number. |
| `ODC-7010` | `collaboration_room_full` | fatal | The room already holds as many participants as it admits, so this connection was not admitted. Terminal for this connection and **not** retryable on it — a client that resends the same join down the same socket spins — but a room's occupancy is a property of a moment, so a later connection may be admitted. Separate from `ODC-7003` because only one of the two is worth waiting out. |
| `ODC-7011` | `collaboration_access_change_refused` | error | A request to change what another participant may do was refused. **Deliberately undetailed**, carrying none of `AccessChangeRefusal`'s five distinctions, for the reason `ODC-7003` gives: what an attacker would be enumerating here is a *ceiling*, one request at a time. Not terminal — the session is otherwise fine, and dropping a co-editing connection over a refused administrative request would cost the reader their unsent work — and not retryable, because nothing about resending it would change the answer. Every one of the five is unreachable from an honest chrome, so a reader never meets this code; the relay returns the distinction for an operator's log. |
| `ODC-8001` | `plugin_failed` | error | A plugin returned an error or violated its declared contract. |
| `ODC-9001` | `internal` | fatal | An unexpected internal failure occurred. |

## Context Policy

Structured context uses allowlisted fields such as:

- `operation`;
- `revision`;
- `node_id`;
- `part_name`;
- `limit_name`;
- `limit_value`;
- `observed_value`;
- `feature`;
- `format`.

Document text, credentials, URLs with tokens, raw XML, and file-system paths are
not included by default.

## Evolution

New codes are appended to the appropriate range. Removing a code requires
retaining a documented tombstone. Bindings expose the string code exactly and
may additionally expose category enums whose unknown variant remains
forward-compatible.
