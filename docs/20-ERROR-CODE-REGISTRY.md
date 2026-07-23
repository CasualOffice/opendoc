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
| `ODC-7001` | `collaboration_conflict` | error | A remote operation cannot be safely applied or rebased. |
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
