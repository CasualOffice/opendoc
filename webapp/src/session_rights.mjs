// SPDX-License-Identifier: Apache-2.0
// Changing what OTHER people may do, inside a shared session and nowhere else.
//
// ---- THE OWNER'S DECISION, WHICH IS THE SPECIFICATION ----------------------
//
//   "In case of co-editing we need a full rights-changing dialog for owner and
//    editor — not viewer. And for SDK or single user, the role is pre-decided
//    while loading the file."
//
// Four consequences, and each one is a line of code somewhere below:
//
//   1. The surface exists only in a ROOM. One doc, one room — a deployed or
//      shared document joins a room even for one reader, and standalone is
//      edit-and-save only. With no room there is nobody else's permission to
//      change, so the command is present and DISABLED WITH A REASON, exactly as
//      `collab.reconnect` is (`collab_chrome.mjs`); the state is legible rather
//      than silent.
//   2. It is offered only to a participant holding `manageAccess`. A viewer sees
//      NOTHING — not a greyed row, not a button.
//   3. It changes other participants' rights, never the reader's own.
//   4. The role itself is never negotiable here. It arrives from the host when
//      the file loads (`capabilities.mjs`) or from the relay on the `Welcome`
//      (`session_access.mjs`), and this module cannot widen either.
//
// ---- WHY (2) IS ABSENCE AND NOT A DISABLED CONTROL -------------------------
//
// `SKILL` §10 says "never a dead control": a command that cannot act ships
// disabled with a reason. Everything else in this editor obeys that, and
// `narrowCommandsToGrant` is the mechanism. This is the one place the owner asked
// for the other answer, in as many words — "not greyed, not present" — and the
// two rules do not actually collide:
//
//   * §10 forbids a control that LOOKS live and does nothing. Absence is not that.
//   * What §10 buys is that a reader learns why. A greyed "Manage access…" tells a
//     viewer what they may NOT do, which is the less useful half; the persistent
//     access indicator (`access_badge.mjs`) tells them what they MAY do, from
//     first paint, in every mode. That is the same requirement answered better.
//
// So: no room → present and disabled with a reason (there is no permission
// question, only an absent session). In a room without `manageAccess` → absent.
// `session_rights.test.mjs` holds both directions.
//
// ---- THE ESTABLISHED PATTERN, NAMED BEFORE ANY CODE ------------------------
//
// **Role-based access control over a capability grant, bounded by delegation.**
// The roles below are not a second permission model — each is a NAMED SUBSET of
// the same capability vocabulary `casual_doc_edit::access` enforces, which is
// what `143` §10 means by "role presets are convenience; the wire carries
// explicit capabilities". The bound is the textbook one for delegation: a
// principal may grant only what it holds, and only within what the grantee was
// already entitled to. `refuse_access_change` is both halves, in the engine.
//
// This module therefore OFFERS what will be accepted and decides nothing. The
// authority is the relay, which re-checks; see that function's own notes for why
// a chrome-side check is a courtesy.
//
// ---- WHAT IS DELIBERATELY NOT HERE ----------------------------------------
//
// Names. A participant is a NUMBER, because that is all the protocol carries:
// `Identity` is opaque to this engine and `ServerMessage::Awareness` reports a
// `ClientId` and nothing else. Inventing a display name would be inventing an
// identity system, which is a separate lane. So the dialog says "Participant 3"
// and says it honestly.
//
// Invitations, link sharing and who may JOIN. Those are the host's half of
// `143` §10's `share.admin` and stay host-side (`access.rs` says so). This
// redistributes rights among the people already in the room.

/** The named roles, ascending by authority, as a capability subset each.
 *
 *  ASCENDING because that is how Google Docs and Word both order a permission
 *  picker, least to most, so the safe answer is the one the eye lands on first.
 *
 *  Each `names` array is sorted the way `PARTICIPANT_CAPABILITIES` is sorted and
 *  the way the engine's `participantCapabilities` getter reports, so a role can be
 *  recognised from a grant by string comparison rather than by set arithmetic —
 *  and `session_rights.test.mjs` asserts each one against the Rust preset it
 *  mirrors (`Capabilities::viewer`, `commenter`, `suggester`, `reviewer`,
 *  `editor`, `owner`) read out of `access.rs`, so a preset that changes in the
 *  engine fails the build here instead of drifting.
 *
 *  `reviewer` is NOT a superset of `suggester`, deliberately, and that asymmetry
 *  is the role: a reviewer resolves suggestions and does not author them. It is
 *  `access.rs`'s own note, mirrored rather than re-derived. */
export const ROLES = Object.freeze([
  Object.freeze({ value: "viewer", labelKey: "rights.role.viewer", names: Object.freeze([]) }),
  Object.freeze({
    value: "commenter",
    labelKey: "rights.role.commenter",
    names: Object.freeze(["comment"]),
  }),
  Object.freeze({
    value: "suggester",
    labelKey: "rights.role.suggester",
    names: Object.freeze(["comment", "suggest"]),
  }),
  Object.freeze({
    value: "reviewer",
    labelKey: "rights.role.reviewer",
    names: Object.freeze(["comment", "review"]),
  }),
  Object.freeze({
    value: "editor",
    labelKey: "rights.role.editor",
    names: Object.freeze(["comment", "edit", "manageAccess", "review", "suggest"]),
  }),
  Object.freeze({
    value: "owner",
    labelKey: "rights.role.owner",
    names: Object.freeze([
      "comment",
      "edit",
      "manageAccess",
      "manageProtection",
      "review",
      "suggest",
    ]),
  }),
]);

/** The catalogue key for a grant that is not one of [`ROLES`].
 *
 *  A real state rather than a defensive branch: `143` §10 exists so a host can
 *  narrow a role without inventing a role vocabulary, and
 *  `Capabilities::narrowed_to` will happily produce `comment + manageProtection`.
 *  Naming it "Custom" is the honest answer; picking the nearest role would report
 *  a grant the participant does not hold. */
export const CUSTOM_ROLE_KEY = "rights.role.custom";

/** A capability name list, sorted and de-duplicated the one way this module reads
 *  them. O(n log n). */
const normalise = (names) =>
  [...new Set((Array.isArray(names) ? names : []).map((name) => String(name)))].sort();

/** Whether `names` is a subset of `bound`. O(n·m), over sets of at most six. */
const within = (names, bound) => names.every((name) => bound.includes(name));

/**
 * The role `names` denotes, or `null` when the grant is a composition no role has.
 *
 * Exact set equality, never "nearest": a participant shown a role they do not hold
 * is a participant told the wrong thing about their own access, which is the whole
 * failure `access.rs`'s `session.*`/`document.*` split exists to prevent one layer
 * down.
 *
 * Complexity: O(roles × names) — six by six.
 *
 * @param {readonly string[]} names
 * @returns {{value: string, labelKey: string, names: readonly string[]}|null}
 */
export function roleOf(names) {
  const wanted = normalise(names).join(",");
  return ROLES.find((role) => [...role.names].join(",") === wanted) ?? null;
}

/**
 * The catalogue key naming what `names` amounts to, role or custom.
 *
 * Complexity: O(roles × names).
 *
 * @param {readonly string[]} names
 * @returns {string}
 */
export function roleKey(names) {
  return roleOf(names)?.labelKey ?? CUSTOM_ROLE_KEY;
}

/**
 * The roles a rights surface may OFFER for one participant.
 *
 * Both delegation bounds, applied here so the surface cannot show a row the relay
 * will refuse — the "never a dead control" rule at a distance, where the control
 * looks live and the refusal arrives from the network. The authority is still
 * `refuse_access_change`, which applies the identical pair; this is the courtesy
 * half, and `session_rights.test.mjs` drives the two against each other.
 *
 *   * `ceiling` — the target's own host-signed grant, from the `Welcome`'s
 *     membership list. Nothing in a room may exceed it.
 *   * `actor` — what the reader holds. Nobody hands out access they do not have.
 *
 * Returns rows in [`ROLES`] order, so a picker's order is the vocabulary's order
 * and not an accident of the intersection.
 *
 * Complexity: O(roles × names).
 *
 * @param {readonly string[]} ceiling
 * @param {readonly string[]} actor
 * @returns {readonly {value: string, labelKey: string, names: readonly string[]}[]}
 */
export function rolesWithin(ceiling, actor) {
  const bound = normalise(ceiling);
  const mine = normalise(actor);
  return ROLES.filter((role) => within([...role.names], bound) && within([...role.names], mine));
}

/**
 * Whether the rights surface is offered at all, and the sentence when it is not.
 *
 * THE ONE PLACE (1) AND (2) FROM THE HEADER ARE DECIDED, so the ribbon, the menu
 * and the palette cannot disagree about it — which is the defect
 * `opendoc-command-surface-parity` records as recurring here.
 *
 *   * `present: false` — a room where this participant holds no `manageAccess`.
 *     The owner's decision: "not greyed, not present." Nothing is rendered.
 *   * `present: true, reasonKey: <key>` — there is no room, so the command ships
 *     DISABLED WITH A REASON. The absent thing is a session, not a permission.
 *   * `present: true, reasonKey: null` — offered.
 *
 * A room of one is offered and `members` is empty, which the dialog says
 * (`rights.empty`) rather than refusing to open: "nobody else is here" is
 * information, and a control that refuses to open cannot deliver it.
 *
 * Complexity: O(1).
 *
 * @param {{shared: boolean, names: readonly string[]}|null} grant the participant grant
 * @returns {{present: boolean, reasonKey: string|null}}
 */
export function rightsSurface(grant) {
  if (!grant?.shared) return Object.freeze({ present: true, reasonKey: "rights.standalone" });
  const held = grant.names ?? [];
  if (!held.includes("manageAccess")) return Object.freeze({ present: false, reasonKey: null });
  return Object.freeze({ present: true, reasonKey: null });
}

/**
 * The room's membership as the dialog renders it: one row per participant.
 *
 * Takes the `members` array a `welcome`/`resumed` outcome carried — already
 * filtered by the relay to participants who may act on it, and already excluding
 * the reader's own row — and adds the two things only this side can decide: which
 * roles to offer, and what to call the grant they hold now.
 *
 * A member whose ceiling admits no role but `viewer` is still a ROW, not an
 * omission: it says truthfully that this person is a viewer and cannot be
 * anything else, which is what a reader opening the dialog wants to know. An
 * omission would read as "they are not here".
 *
 * Complexity: O(members × roles).
 *
 * @param {readonly object[]} members
 * @param {readonly string[]} actor the reader's own capability names
 * @returns {readonly object[]}
 */
export function membershipRows(members, actor) {
  return Object.freeze(
    (Array.isArray(members) ? members : [])
      .map((member) => {
        const participant = Number(member?.participant);
        if (!Number.isSafeInteger(participant) || participant < 0) return null;
        const held = normalise(member?.capabilities);
        const ceiling = normalise(member?.ceiling);
        return Object.freeze({
          participant,
          held,
          ceiling,
          /** What they are now, which may be `rights.role.custom`. */
          roleKey: roleKey(held),
          /** The role whose names equal `held`, or `null` for a custom grant —
           *  which a picker shows as having no selection rather than guessing. */
          role: roleOf(held)?.value ?? null,
          options: rolesWithin(ceiling, actor),
        });
      })
      .filter((row) => row !== null)
      .sort((a, b) => a.participant - b.participant),
  );
}

/**
 * The capability names a role denotes, or `null` for a value no role has.
 *
 * `null` and not an empty array: an empty array is `viewer`, a real and very
 * different answer, and a typo that silently demoted somebody to a viewer is the
 * worst failure this module could have.
 *
 * Complexity: O(roles).
 *
 * @param {string} value
 * @returns {readonly string[]|null}
 */
export function namesForRole(value) {
  return ROLES.find((role) => role.value === String(value))?.names ?? null;
}

/**
 * Mounts the rights dialog over the markup in `editor.html`.
 *
 * Returns the same shape whether or not the markup is there, so a host embedding
 * a cut-down shell gets an inert object rather than a throw — the pattern
 * `document_protection.mjs` established and for its reason.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc the engine handle
 * @param {() => object|null} io.getTransport the shared session, or `null`
 * @param {() => {shared: boolean, names: readonly string[]}|null} io.getGrant
 * @param {() => readonly object[]} io.getMembers the last membership list seen
 * @param {(dialog: Element, options: object) => object} io.registerModal
 * @param {() => Element|null} io.fallbackFocus
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {(key: string, values?: object) => string} io.t
 */
export function createSessionRights(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("sessionRightsDialog");
  const list = el("sessionRightsList");
  const empty = el("sessionRightsEmpty");
  const apply = el("sessionRightsApply");
  const close = el("sessionRightsClose");
  const cancel = el("sessionRightsCancel");
  const inert = {
    open() {},
    reflect() {},
    commands: () => [],
    isPresent: () => false,
  };
  if (!dialog || !list || !apply) return inert;

  /** The role each row is set to while the dialog is open, by participant number.
   *  Held here and not read back off the DOM, because Apply has to know what
   *  CHANGED — sending every row would re-assert a grant nobody touched, and the
   *  relay would answer each one with a fan-out. */
  let chosen = new Map();

  const modal = io.registerModal(dialog, {
    initialFocus: () => list.querySelector("select") ?? close ?? apply,
    fallbackFocus: io.fallbackFocus,
    defaultAction: () => void applyChanges(),
  });

  /** One row: who, what they are, and what they may become. */
  function render() {
    const grant = io.getGrant?.() ?? null;
    const rows = membershipRows(io.getMembers?.() ?? [], grant?.names ?? []);
    list.textContent = "";
    chosen = new Map();
    for (const row of rows) {
      const line = document.createElement("div");
      line.className = "rights-row";
      line.dataset.participant = String(row.participant);

      const who = document.createElement("span");
      who.className = "rights-who";
      // A NUMBER, because that is all the protocol carries — see the header.
      who.textContent = io.t("rights.participant", { number: row.participant });
      line.append(who);

      const picker = document.createElement("select");
      picker.className = "rights-role";
      picker.setAttribute("aria-label", io.t("rights.participant", { number: row.participant }));
      // A custom grant gets a first option naming itself, selected and disabled:
      // the reader is told what the participant holds without being offered a
      // value that is not a role.
      if (row.role === null) {
        const custom = document.createElement("option");
        custom.textContent = io.t(row.roleKey);
        custom.selected = true;
        custom.disabled = true;
        picker.append(custom);
      }
      for (const option of row.options) {
        const node = document.createElement("option");
        node.value = option.value;
        node.textContent = io.t(option.labelKey);
        node.selected = option.value === row.role;
        picker.append(node);
      }
      // A participant whose ceiling admits exactly one role cannot be moved, so
      // the picker says so instead of offering a gesture with one outcome.
      picker.disabled = row.options.length <= 1 && row.role !== null;
      picker.addEventListener("change", () => {
        chosen.set(row.participant, picker.value);
      });
      line.append(picker);
      list.append(line);
    }
    if (empty) empty.hidden = rows.length > 0;
    apply.disabled = rows.length === 0;
  }

  /** Sends one frame per CHANGED row, through the engine, over the transport.
   *
   *  The frame is built by `collabSetAccessFrame` and not here: nothing in
   *  `webapp/src` composes a wire frame (ADR-063), and the engine is also where a
   *  bad participant number or capability name is refused with a sentence. */
  function applyChanges() {
    const doc = io.getDoc?.();
    const pipe = io.getTransport?.();
    if (!doc || !pipe) {
      io.setStatus(io.t("rights.standalone"), "warn");
      modal.close();
      return;
    }
    const rows = membershipRows(io.getMembers?.() ?? [], io.getGrant?.()?.names ?? []);
    let sent = 0;
    for (const row of rows) {
      const want = chosen.get(row.participant);
      if (want === undefined || want === row.role) continue;
      const names = namesForRole(want);
      if (names === null) continue;
      let frame;
      try {
        frame = doc.collabSetAccessFrame(row.participant, [...names]);
      } catch (error) {
        // The engine's own sentence, kept verbatim: it names what it refused,
        // which nothing here could.
        io.setStatus(String(error?.message ?? error), "error");
        return;
      }
      if (!pipe.request(frame)) {
        io.setStatus(io.t("rights.notSent"), "warn");
        return;
      }
      sent += 1;
    }
    modal.close();
    // Deliberately SILENT on success, and the silence is the point: the relay
    // decides, and the reader is told what actually happened when the
    // `accessChanged` frame comes back — `rights.applied`, naming who and what.
    // Announcing here would be reporting the request and calling it the result,
    // which is the "fixed means merged" confusion one layer down.
    void sent;
  }

  close?.addEventListener("click", () => modal.close());
  cancel?.addEventListener("click", () => modal.close());
  apply.addEventListener("click", () => void applyChanges());

  /** Named rather than a method, so the command row's `run` cannot depend on how
   *  `commands()` happened to be called. */
  function openDialog() {
    if (!rightsSurface(io.getGrant?.() ?? null).present) return;
    render();
    modal.open();
  }

  return Object.freeze({
    /** Opens the dialog over the membership the session last reported. */
    open: openDialog,
    /** Re-renders while open, so an `accessChanged` from anybody keeps the list
     *  true rather than showing a role somebody has already left. */
    reflect() {
      if (!modal.isOpen) return;
      render();
    },
    /** Whether any surface should carry this command at all. */
    isPresent: () => rightsSurface(io.getGrant?.() ?? null).present,
    /** The one command row, generated here so the palette and the menu cannot be
     *  offered different sets.
     *
     *  **An empty array is the viewer's answer**, and it is what makes (2) true
     *  everywhere at once: the registry has no row, so there is nothing for the
     *  palette, the menu or the ribbon sweep to show. */
    commands() {
      const surface = rightsSurface(io.getGrant?.() ?? null);
      if (!surface.present) return [];
      const reason = surface.reasonKey ? io.t(surface.reasonKey) : "";
      return [
        {
          id: "review.manageAccess",
          label: io.t("rights.command"),
          group: "Review",
          kw: "manage access rights permission permissions role roles share sharing participant participants owner editor viewer commenter reviewer collaborator who can edit",
          enabled: !reason,
          disabledReason: reason,
          run: openDialog,
        },
      ];
    },
  });
}
