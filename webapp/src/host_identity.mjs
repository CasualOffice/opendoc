// SPDX-License-Identifier: Apache-2.0
// Who this editor's person IS, when the host says so (`docs/175` §3, ADR-070).
//
// The fourth host input. `host_contract.mjs` owns its SHAPE (`validateHostIdentity`,
// the one validator every transport calls); this module owns its LIFE in one page:
//
//   * it is decided ONCE. `identify` is accepted a single time, and never after a
//     shared room has been joined: a mid-session change would put two people's
//     work in one undo history and one resume key (`docs/175` §3.2). A second
//     call is refused `identity-fixed`;
//   * when it is given it is the ONLY identity (§3.1). `activeAuthor` answers
//     "who is writing" for every consumer — the engine's active author, the
//     version timeline's actor, the room join — so none of them reads the
//     Settings name behind its back;
//   * it never travels in a URL. The element and a `postMessage` host hand it
//     over in a one-shot message; a host that will do so says only THAT it will,
//     with `awaitIdentity=1`, and the editor holds its startup open until the
//     identity lands, so a startup document or room is never opened as somebody
//     else first.
//
// The avatar policy lives here too, because it is the other per-mount fact about
// people: `avatars=fetch` is the host's consent to load a picture URL (§8.1), and
// absent it nothing is fetched.
//
// Pure apart from one page-level holder (`hostIdentity`): no DOM, no engine, no
// `t()`. Every function below takes its inputs, so node drives all of it.
import { validateHostIdentity } from "./host_contract.mjs";

/** The flag a host sets when it WILL send an identity. Carries no identity. */
export const AWAIT_IDENTITY_PARAM = "awaitIdentity";
/** The avatar-consent parameter the element writes (`avatars="fetch"`). */
export const AVATARS_PARAM = "avatars";
/** How long a startup waits for a promised identity before opening anyway. */
export const IDENTITY_WAIT_MS = 10_000;

/**
 * One page's identity, decided once.
 *
 * Complexity: O(1) per call, plus the validator's O(identity size).
 */
export function createIdentityHolder({ setTimer = setTimeout, clearTimer = clearTimeout } = {}) {
  let identity = null;
  let decided = false;
  let joined = false;
  const listeners = new Set();
  const waiters = new Set();
  const fixed = (message) =>
    Object.freeze({
      ok: false,
      refusal: Object.freeze({ code: "identity-fixed", command: "identify", requires: null, message, field: "identity" }),
    });
  return {
    /** The validated identity, or null when the host gave none. Frozen. */
    get identity() {
      return identity;
    },
    /** Accepts the host's identity, once. Returns `{ ok, notes }` or
     *  `{ ok: false, refusal }` — a value, never a throw, so both transports
     *  answer a host the same way they answer a refused command. */
    identify(raw) {
      if (decided) return fixed("The identity was already given; it is decided once per mount.");
      if (joined) return fixed("A shared room was already joined; an identity cannot change inside one.");
      const checked = validateHostIdentity(raw);
      if (!checked.ok) return checked;
      decided = true;
      identity = checked.identity;
      for (const waiter of waiters) waiter();
      waiters.clear();
      for (const listener of listeners) {
        try {
          listener(identity);
        } catch (err) {
          console.warn("opendoc: an identity listener threw", err);
        }
      }
      return Object.freeze({ ok: true, notes: checked.notes });
    },
    /** A shared room is being joined: from here the identity is fixed, given or not. */
    markJoined() {
      joined = true;
    },
    /** Calls `listener(identity)` when the identity arrives. Returns an unsubscribe. */
    onChange(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    /** Resolves when an identity has arrived, or after `ms` — never rejects, so a
     *  host that promised one and never sent it costs a wait, not an editor. */
    settled(ms = IDENTITY_WAIT_MS) {
      if (decided) return Promise.resolve(identity);
      return new Promise((resolve) => {
        const done = () => {
          clearTimer(timer);
          waiters.delete(done);
          resolve(identity);
        };
        const timer = setTimer(done, ms);
        waiters.add(done);
      });
    },
  };
}

/** The page's one holder. The in-process session and the `postMessage` bridge
 *  both reach it through `createHostSession`'s default `identify`. */
export const hostIdentity = createIdentityHolder();

/** Whether the URL says a host identity is on its way (`awaitIdentity=1`). */
export function identityPromised(search) {
  try {
    return new URLSearchParams(search ?? "").get(AWAIT_IDENTITY_PARAM) === "1";
  } catch {
    return false;
  }
}

/** The mount's avatar policy: `"fetch"` only when the host consented, else
 *  `"never"` (`docs/175` §8.1). */
export function avatarPolicy(search) {
  try {
    return new URLSearchParams(search ?? "").get(AVATARS_PARAM) === "fetch" ? "fetch" : "never";
  } catch {
    return "never";
  }
}

/**
 * Who is writing: `{ name, initials, id, persist, fixed }`.
 *
 * The host identity when there is one (`fixed: true` — Settings cannot change
 * it), otherwise the Settings name, otherwise the anonymous "You" the owner kept
 * (`docs/175` §10.1). `id` and `persist` exist only for a host identity: a typed
 * name is never an id, and nothing enters a saved file without the host's
 * `persist` (§6.2).
 *
 * Complexity: O(1).
 */
export function activeAuthor(settings, identity = hostIdentity.identity) {
  if (identity) {
    return Object.freeze({
      name: identity.name,
      initials: identity.initials,
      id: identity.id,
      persist: identity.persist ?? null,
      fixed: true,
    });
  }
  const name = String(settings?.authorName ?? "").trim();
  const initials = String(settings?.authorInitials ?? "").trim();
  return Object.freeze({ name: name || "You", initials: initials || undefined, id: undefined, persist: null, fixed: false });
}

/**
 * Reflects who is writing onto Settings' two author fields (`docs/175` §3.1).
 *
 * With a host identity both fields show it, are DISABLED, and say why: the
 * `reason` element (its sentence is markup, so it is localised by the same pass
 * as the rest of the panel and nothing calls `t()` at boot, SKILL §11) is shown,
 * named by each field's `aria-describedby`, and copied into each `title` for a
 * pointer. Without one they are the visitor's own, editable, and the reason is
 * hidden and un-referenced — a hidden description that is still referenced is
 * still read aloud. Never a dead control (SKILL §10).
 *
 * Takes the elements as arguments, so node drives it with plain objects.
 * Complexity: O(1).
 */
export function reflectAuthorIdentity({ nameInput, initialsInput, reason, settings, identity = hostIdentity.identity }) {
  const fixed = Boolean(identity);
  if (reason) reason.hidden = !fixed;
  const why = fixed ? String(reason?.textContent ?? "").trim() : "";
  for (const input of [nameInput, initialsInput]) {
    if (!input) continue;
    input.disabled = fixed;
    if (fixed) {
      input.title = why;
      if (reason?.id) input.setAttribute("aria-describedby", reason.id);
    } else {
      input.removeAttribute("title");
      input.removeAttribute("aria-describedby");
    }
  }
  if (nameInput) nameInput.value = fixed ? identity.name : String(settings?.authorName ?? "");
  if (initialsInput) initialsInput.value = fixed ? (identity.initials ?? "") : String(settings?.authorInitials ?? "");
}

/** The name a version or a field records: the host identity's, else the
 *  Settings name, else "" (a version with no author says nothing rather than
 *  inventing "You"). */
export function authorNameFor(settings, identity = hostIdentity.identity) {
  return identity ? identity.name : String(settings?.authorName ?? "").trim();
}

/**
 * What a room join says about this person: `{ identity, profile }`, where
 * `identity` is the opaque join string and `profile` the JSON the engine's
 * `collabJoinFrame` takes, or undefined.
 *
 *   * a host identity joins as its `id`, with its name, initials, colour and
 *     avatar URL as the profile — attested if a grant vouches for it, claimed
 *     (and shown unverified, colour and avatar dropped by the relay) if not;
 *   * no host identity but a Settings name: joins as that name, as today, with a
 *     CLAIMED profile `{ id: <the same string>, name }`;
 *   * neither: joins as "You" with no profile, and the room shows the honest
 *     "Participant N" (`docs/175` §10).
 *
 * Complexity: O(1).
 */
export function joinIdentity(settings, identity = hostIdentity.identity) {
  if (identity) {
    const profile = { id: identity.id, name: identity.name };
    if (identity.initials) profile.initials = identity.initials;
    if (identity.color) profile.color = identity.color;
    if (identity.avatar?.url) profile.avatarUrl = identity.avatar.url;
    return Object.freeze({ identity: identity.id, profile: JSON.stringify(profile) });
  }
  const name = String(settings?.authorName ?? "").trim();
  if (name) return Object.freeze({ identity: name, profile: JSON.stringify({ id: name, name }) });
  return Object.freeze({ identity: "You", profile: undefined });
}
