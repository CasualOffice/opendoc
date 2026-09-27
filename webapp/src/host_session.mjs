// The IN-PROCESS transport: a host holding a reference to the editor.
//
// `docs/126` phase 2. This is the session object a host calls directly —
// `session.execute("format.bold")` — and it is also the only thing
// `host_bridge.mjs` calls. The `postMessage` transport is a thin envelope over
// THIS, not a second implementation of it, which is what makes "identical
// whether they hold a reference or are across an iframe boundary" a structural
// fact rather than a pair of tables somebody keeps in step.
//
// Everything it knows about commands, events, refusals and gating comes from
// `host_contract.mjs`. It holds no command list of its own.
//
// FOUR PROPERTIES THIS FILE EXISTS TO HOLD:
//
//  1. **Capabilities gate the API, not just the chrome.** The gate runs BEFORE
//     dispatch, so a host that was not granted `edit` never reaches `run()`.
//     Phase 1 layered browser sandbox, engine mode and chrome; this is the
//     fourth door and it must not be the unlocked one.
//  2. **A refused command says so in its RESULT.** #632 fixed exactly this class
//     of lie in the chrome — Save ran, no download fired, the status line said
//     "Saved" — and reintroducing it at the API layer would be the same defect
//     one level up. So `execute` returns `{ok: false, refusal}` and a host can
//     branch on `refusal.code`.
//  3. **A refusal the ENGINE makes is reported too.** Viewing mode refuses at
//     one fail-closed choke point and says so through `setStatus(…, "error")`.
//     Rather than guessing, this watches that channel across the command and
//     reports what the editor actually said — `status_channel.mjs` already
//     distinguishes a refusal from a confirmation, and `announcementRegion` is
//     that distinction, so this asks it instead of copying it.
//  4. **Per-interaction work is O(1) in document size** (`docs/107` §4). `change`
//     carries a revision handle and a dirty flag, never a snapshot; `selection`
//     carries two positions. Nothing here walks the document.
//
// No English: every sentence a refusal carries is either the chrome's own
// localised `disabledReason`, the editor's own status text, or the
// `withheldMessage` this factory is handed. A module that takes its labels as
// input adds no string debt (`docs/124`).
import { editingModeFor } from "./capabilities.mjs";
import {
  CONTRACT_VERSION,
  COMMAND_FAMILIES,
  HOST_EVENT_NAMES,
  PROTOCOL,
  REFUSAL_CODES,
  capabilityRefusal,
  commandContract,
} from "./host_contract.mjs";
import { announcementRegion } from "./status_policy.mjs";

/** A frozen ok result. `revision` is the handle a host correlates against the
 *  `change` event; `null` when the engine could not report one. */
const ok = (id, revision) => Object.freeze({ ok: true, command: id, revision });

/** A frozen refusal result. */
const no = (id, code, message, extra = {}) =>
  Object.freeze({
    ok: false,
    command: id,
    revision: extra.revision ?? null,
    refusal: Object.freeze({ code, command: id, message, requires: extra.requires ?? null }),
  });

/**
 * Creates the in-process session.
 *
 * Every piece of editor state arrives as a FUNCTION, so this module reaches no
 * global and can be driven in node with plain objects — which is what lets the
 * gate, the refusal taxonomy and the event payloads be tested without booting a
 * WebAssembly editor.
 *
 * @param {object} io
 * @param {{has: (name: string) => boolean}} io.capabilities the resolved grant.
 * @param {() => Array<object>} io.registry the live command registry, in the
 *   same surface the command palette and `runCommandById` use — so the API can
 *   reach exactly what a user can, and no more.
 * @param {() => string} [io.withheldMessage] the localised sentence for a
 *   capability the host did not grant.
 * @param {() => number|null} [io.revision] the engine's revision watermark.
 * @param {() => boolean} [io.dirty] whether there is unsaved work.
 * @param {() => object|null} [io.selection] a compact selection descriptor.
 * @param {() => object} [io.document] name and format of the open document.
 */
export function createHostSession({
  capabilities,
  registry,
  withheldMessage = () => "",
  revision = () => null,
  dirty = () => false,
  selection = () => null,
  document: documentInfo = () => ({}),
}) {
  /** event name -> listeners. One Set per event so an `off` is O(1). */
  const listeners = new Map(HOST_EVENT_NAMES.map((name) => [name, new Set()]));
  /** The last refusal the editor published, and a counter so a command can tell
   *  "a refusal happened while I ran" from "a refusal happened earlier". */
  let statusSeq = 0;
  let lastRefusal = null;
  let ready = false;

  function emit(name, detail) {
    const set = listeners.get(name);
    if (!set) return;
    for (const listener of set) {
      try {
        listener(Object.freeze({ event: name, detail: Object.freeze(detail) }));
      } catch (err) {
        // A host's own listener throwing must not take down the editor, and must
        // not be reported as an editor error either — it is theirs.
        console.warn(`opendoc: a ${name} listener threw`, err);
      }
    }
  }

  /** The registry as a Map, built once per call that needs it. O(registry) —
   *  the same build the command palette already performs on every keystroke
   *  typed into its input, so this is an accepted cost on an existing path. */
  function descriptors() {
    const map = new Map();
    let rows = [];
    try {
      rows = registry() ?? [];
    } catch (err) {
      // The registry could not be built in this state, which happens for real: a
      // host may ask before the editor has opened anything, and the editor builds
      // its registry from document state. An empty registry answers honestly —
      // every command reads as `present: false`, and `execute` refuses with
      // `unknown-command` — where a throw would let one early call take down the
      // embed.
      console.warn("opendoc: the command registry is not available yet", err);
      rows = [];
    }
    for (const command of rows) map.set(command.id, command);
    return map;
  }

  /** What `query` answers, for one id against one registry snapshot. */
  function describeCommand(id, map) {
    const row = commandContract(id);
    const descriptor = map.get(id);
    const granted = row ? capabilityRefusal(row, capabilities, "") === null : false;
    return Object.freeze({
      id,
      /** In the contract at all. */
      known: !!row,
      /** In the registry right now. A command can be known and absent: the table
       *  band's commands exist only while the caret is in a table. */
      present: !!descriptor,
      /** What the host would have to have been granted. */
      requires: row?.requires ?? null,
      /** Whether this host was granted it. */
      granted,
      /** Whether it could run right now: granted, present, and not disabled. */
      available: granted && !!descriptor && descriptor.enabled !== false,
      /** The chrome's own reason, when there is one. Already localised. */
      reason: descriptor && descriptor.enabled === false ? (descriptor.disabledReason ?? "") : "",
    });
  }

  const session = {
    /** The contract, resolved against the live registry.
     *
     *  This is what a host enumerates to build its own UI, and what the
     *  `postMessage` client uses to generate its method surface — so a host never
     *  hard-codes a command list either.
     *
     *  Complexity: O(registry). Called on connect, not per interaction. */
    describe() {
      const map = descriptors();
      return Object.freeze({
        contract: CONTRACT_VERSION,
        protocol: PROTOCOL.version,
        events: HOST_EVENT_NAMES,
        refusalCodes: REFUSAL_CODES,
        capabilities: Object.freeze([...capabilities]),
        editingMode: editingModeFor(capabilities),
        families: COMMAND_FAMILIES,
        document: Object.freeze({ ...documentInfo() }),
        commands: Object.freeze([...map.keys()].sort().map((id) => describeCommand(id, map))),
      });
    },

    /** The state of one command. `docs/05` §5's `query_command`.
     *
     *  Complexity: O(registry), as `describe`. */
    query(id) {
      return describeCommand(String(id ?? ""), descriptors());
    },

    /** Liveness, and the cheapest possible one: no registry build. */
    ping() {
      return Object.freeze({ contract: CONTRACT_VERSION, ready, revision: revision() });
    },

    /**
     * Runs a command by id and reports what happened.
     *
     * The order is the whole design:
     *
     *   1. the CONTRACT — an id outside it is `unknown-command`, so a host cannot
     *      reach a registry row the contract has not declared;
     *   2. the CAPABILITY — refused before dispatch, so the grant is enforced by
     *      the API and not only by the chrome;
     *   3. the REGISTRY — absent is `unknown-command` in this state, disabled is
     *      `unavailable` with the chrome's own reason;
     *   4. the ENGINE — it runs, and a refusal it publishes to the status channel
     *      comes back as `engine-refused`.
     *
     * Returns a promise because several registry commands are async; the gate and
     * the dispatch decision are synchronous, so a refusal never depends on
     * timing.
     *
     * Complexity: O(registry) for the lookup plus whatever the command itself
     * costs. Nothing here is per-keystroke.
     */
    async execute(id, args = []) {
      const commandId = String(id ?? "");
      const row = commandContract(commandId);
      if (!row) return refuse(no(commandId, "unknown-command", ""));
      const withheld = capabilityRefusal(row, capabilities, withheldMessage());
      if (withheld) {
        return refuse(
          Object.freeze({
            ok: false,
            command: commandId,
            revision: revision(),
            refusal: withheld,
          }),
        );
      }
      const descriptor = descriptors().get(commandId);
      if (!descriptor) return refuse(no(commandId, "unknown-command", ""));
      if (descriptor.enabled === false) {
        return refuse(
          no(commandId, "unavailable", descriptor.disabledReason ?? "", { revision: revision() }),
        );
      }
      const before = statusSeq;
      try {
        await descriptor.run(...(Array.isArray(args) ? args : [args]));
      } catch (err) {
        return refuse(
          no(commandId, "threw", String(err?.message ?? err), { revision: revision() }),
          true,
        );
      }
      // Did the editor refuse it while it ran? `lastRefusal` is only set by a
      // status the editor itself classified as a failure, so this reports the
      // editor's own word rather than inferring one from a revision that did not
      // move (a command may legitimately change nothing).
      if (statusSeq > before && lastRefusal) {
        return refuse(
          no(commandId, "engine-refused", lastRefusal, { revision: revision() }),
        );
      }
      return ok(commandId, revision());
    },

    /** Subscribes to one event. Returns an unsubscribe, so a host never has to
     *  keep the function reference to stop listening. */
    on(name, listener) {
      const set = listeners.get(name);
      if (!set || typeof listener !== "function") return () => {};
      set.add(listener);
      return () => set.delete(listener);
    },

    /** Unsubscribes. */
    off(name, listener) {
      listeners.get(name)?.delete(listener);
    },

    // ---- What the editor tells the session -----------------------------------
    //
    // Five notifications, and they are the only lines this contract costs
    // `main.js`. Each one is an O(1) hook on a choke point the editor already
    // has, which is why they are notifications rather than the session polling
    // for changes.

    /** The document is open and painted. Emits `ready` once per session; a
     *  second call is ignored, because a host that hears `ready` twice cannot
     *  tell a reopen from a bug. */
    noteReady() {
      if (ready) return;
      ready = true;
      const described = session.describe();
      emit("ready", {
        contract: CONTRACT_VERSION,
        capabilities: described.capabilities,
        editingMode: described.editingMode,
        commands: described.commands.length,
      });
    },

    /** Every status the editor publishes, refusal or not.
     *
     *  Hooked into `setStatus`, which is the editor's single feedback entry point
     *  (~124 call sites, all through it since `109` UX-017). `announcementRegion`
     *  decides which of the two this is — the same function the live regions and
     *  the toast already ask — so "what counts as a refusal" has one answer.
     *
     *  Complexity: O(1) per message, and messages arrive at human rate. */
    noteStatus(text, kind = "") {
      const failed = announcementRegion(kind) === "assertive";
      statusSeq += 1;
      lastRefusal = failed ? String(text ?? "") : null;
      if (!failed || !text) return;
      emit("refusal", {
        code: "engine-refused",
        command: null,
        requires: null,
        message: String(text),
      });
    },

    /** An edit landed. Hooked into `noteDocumentEdited`, the one choke point
     *  every applied edit passes.
     *
     *  Carries the revision watermark and the dirty flag and NOTHING ELSE. A
     *  `change` event that serialised the document would make every keystroke
     *  O(document), which is a defect however fast it feels on a fixture
     *  (`docs/107` §4). A host that wants the bytes runs an export. */
    noteChange() {
      emit("change", { revision: revision(), dirty: dirty() });
    },

    /** The caret or the selection moved. Hooked into `drawSelection`.
     *
     *  Two positions, each a node handle and a grapheme offset — O(1), and the
     *  same anchors `docs/45` I3 makes the stable addressing scheme. */
    noteSelection() {
      const current = selection();
      if (!current) return;
      emit("selection", current);
    },

    /** Bytes left the editor. `intent` is `"save"` or `"export"`, decided by the
     *  caller because only the call site knows which the user asked for — File ▸
     *  Save and File ▸ Export as DOCX can produce identical bytes and are not the
     *  same event.
     *
     *  Carries the byte COUNT, not the bytes: the visitor already has the file,
     *  and posting a document across a frame boundary on every save is exactly
     *  the payload this contract refuses to send. */
    noteWrite(intent, detail) {
      if (intent !== "save" && intent !== "export") return;
      emit(intent, {
        format: String(detail?.format ?? ""),
        name: String(detail?.name ?? ""),
        bytes: Number(detail?.bytes ?? 0),
      });
    },

    /** Something went wrong that was not a command's refusal. */
    noteError(code, message, command = null) {
      emit("error", { code: String(code), message: String(message ?? ""), command });
    },
  };

  /** Emits the `refusal` event for a result and returns the result.
   *
   *  One place, so a refusal can never be returned without also being announced
   *  — a host that only listens and a host that only awaits must learn the same
   *  thing. `alsoError` marks the exception case, which is both. */
  function refuse(result, alsoError = false) {
    emit("refusal", result.refusal);
    if (alsoError) {
      emit("error", {
        code: result.refusal.code,
        message: result.refusal.message,
        command: result.command,
      });
    }
    return result;
  }

  return session;
}
