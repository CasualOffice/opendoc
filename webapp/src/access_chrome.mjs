// SPDX-License-Identifier: Apache-2.0
// The one seam `main.js` holds for "who may do what": the rights dialog, the
// persistent access indicator, and the session messages that move either.
//
// ---- WHY A THIRD MODULE AND NOT TWO IMPORTS -------------------------------
//
// Because `main.js` is at its line ceiling (`module_seams.test.mjs`) and because
// the two halves are one question asked twice. `session_rights.mjs` answers "may
// I change somebody else's access, and to what"; `access_badge.mjs` answers "what
// may I do, and who decided". They read the SAME four authorities — the engine's
// refusal, the container's grant, the participant's grant, the document's own
// policy — and a caller that wired them separately would be the second place that
// knew how to compose those four. The recurring defect this repository records is
// exactly that: a capability resolved independently at six call sites is a
// capability six call sites can disagree about (`capabilities.mjs`'s own note).
//
// So this is the composition, and `main.js` holds one handle.
//
// ---- THE MEMBERSHIP LIST IS STATE, AND IT LIVES HERE ----------------------
//
// A `welcome` or a `resumed` carries the room's membership; an `accessChanged`
// moves one row of it. Nothing else in the chrome wants it, and `main.js` holding
// it would be `main.js` holding protocol state — which is the line ADR-063 draws
// (nothing in `webapp/src` parses a frame; this does not either, it reads the
// outcome the engine already decoded).
//
// It is replaced wholesale by a `welcome`/`resumed` and never merged, for `152`
// §2b's reason about presence: a list built by merging updates onto a list from a
// previous connection is a list that can hold somebody who left while the socket
// was away.

import { createAccessBadge } from "./access_badge.mjs";
import { createSessionRights, rightsSurface, roleKey } from "./session_rights.mjs";

/**
 * Mounts the access surfaces over the markup in `editor.html`.
 *
 * Returns the same shape whether or not the markup is there, so a host embedding
 * a cut-down shell gets an inert handle rather than a throw.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc the engine handle
 * @param {() => object|null} io.getTransport the shared session, or `null`
 * @param {() => {shared: boolean, names: readonly string[]}|null} io.getGrant
 * @param {() => {active?: boolean, value?: string}|null} io.getProtection
 * @param {() => {has?: (name: string) => boolean}|null} io.getCapabilities
 * @param {() => string} io.getReadOnlyReason
 * @param {(dialog: Element, options: object) => object} io.registerModal
 * @param {() => Element|null} io.fallbackFocus
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {(key: string, values?: object) => string} io.t
 */
export function createAccessChrome(io) {
  /** The room's membership, as the last `welcome`/`resumed` reported it.
   *
   *  Empty is a real answer in three different situations and the surface tells
   *  them apart from the GRANT rather than from this: a room of one, a
   *  participant the relay judged unable to act on the list, and no room at all. */
  let members = [];

  const rights = createSessionRights({
    getDoc: io.getDoc,
    getTransport: io.getTransport,
    getGrant: io.getGrant,
    getMembers: () => members,
    registerModal: io.registerModal,
    fallbackFocus: io.fallbackFocus,
    setStatus: io.setStatus,
    t: io.t,
  });

  const reflectBadge = createAccessBadge({
    badge: document.getElementById("accessBadge"),
    level: document.getElementById("accessBadgeLevel"),
    source: document.getElementById("accessBadgeSource"),
    t: io.t,
  });

  const button = document.getElementById("reviewManageAccessBtn");

  /** The badge and the ribbon face, from the four authorities, in one place.
   *
   *  O(1). Called from `updateToolbar`, which is the per-interaction sweep, so it
   *  may not do anything proportional to the document — and it does not: every
   *  input is a field read. */
  function reflect() {
    reflectBadge({
      readOnlyReason: io.getReadOnlyReason?.() ?? "",
      protection: io.getProtection?.() ?? null,
      grant: io.getGrant?.() ?? null,
      capabilities: io.getCapabilities?.() ?? null,
    });
    if (!button) return;
    // ABSENT, not disabled, for a participant whose grant withholds it — the
    // owner's decision, "not greyed, not present". `session_rights.mjs`'s header
    // carries why that does not break "never a dead control", and the ONE place
    // that decides it is `rightsSurface`, so this face and the command row cannot
    // disagree.
    const surface = rightsSurface(io.getGrant?.() ?? null);
    button.hidden = !surface.present;
    // Outside a room it is present and disabled WITH A REASON: the absent thing
    // is a session, not a permission, and that distinction is the whole of what
    // makes the standalone state legible rather than silent.
    button.disabled = surface.reasonKey !== null || !io.getDoc?.();
    const reason = surface.reasonKey
      ? io.t(surface.reasonKey)
      : io.getDoc?.()
        ? ""
        : io.t("command.needsDocument");
    if (reason) button.setAttribute("title", reason);
    else button.removeAttribute("title");
    rights.reflect();
  }

  return Object.freeze({
    /** The command row(s) the registry splices in — empty for a participant the
     *  surface is not offered to, which is what makes the absence true on every
     *  surface at once. */
    commands: () => rights.commands(),
    /** Opens the dialog. The `REVIEW_SURFACE` row's `run`. */
    open: () => rights.open(),
    /** Re-reads every authority and repaints both surfaces. */
    reflect,
    /**
     * One session outcome, as `collab_transport.mjs` hands it over.
     *
     * Returns the sentence to announce, or `""`. A STRING and not a side effect,
     * so the caller keeps its one status channel and this module does not become
     * a second thing that writes to it.
     *
     * `accessChanged` about SOMEBODY ELSE is announced too, and deliberately: in
     * a room, somebody else's role changing is news to a manager with the dialog
     * open and to a reader who is about to ask them to do something. The one it
     * is most news to is the person it happened to, and that case is the same
     * sentence — the engine has already narrowed or widened this replica by then
     * (`apply_room_access`), so the role named is the one actually in force.
     */
    received(outcome) {
      const kind = outcome?.kind;
      if (kind === "welcome" || kind === "resumed") {
        // Replaced wholesale, never merged — see the header.
        members = Array.isArray(outcome.members) ? outcome.members : [];
        return "";
      }
      if (kind !== "accessChanged") return "";
      const who = Number(outcome.about);
      const names = Array.isArray(outcome.capabilities) ? outcome.capabilities : [];
      members = members.map((member) =>
        Number(member?.participant) === who ? { ...member, capabilities: names } : member,
      );
      return io.t("rights.applied", {
        who: io.t("rights.participant", { number: who }),
        role: io.t(roleKey(names)),
      });
    },
  });
}
