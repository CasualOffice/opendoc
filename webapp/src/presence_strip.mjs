// Who is in this shared document: the avatar strip at the right of the header,
// immediately before Share (`docs/174` RC-07, `docs/109` HF-330).
//
// Google Docs, Word on the web and ONLYOFFICE all put the people in a document
// in the same place — a row of round, coloured chips at the right end of the
// header, just before Share — and all three make a chip a way to find that
// person: Docs and Word scroll to their cursor, ONLYOFFICE lists them from a
// users button with a count. This is that row.
//
// ONE seam for what a participant looks like: `participantDisplay`, with the
// input and output `docs/175` §14 fixes. Today the room carries no profile at
// all — a participant is the number the relay assigned (`docs/169` §Presence) —
// so a chip says "P2", and its tooltip and accessible name say, in words, that
// the number stands in for a name this host does not share. When profiles
// arrive (their transport is `docs/175`'s, not this file's), they reach the
// chrome as `participant.profile` with `attested`, and nothing below the seam
// changes. The owner's rules for them (`docs/175` §12) are already here:
//
//   * a CLAIMED name (`attested` false) is shown, marked unverified in its style,
//     tooltip and accessible name, and never with an avatar image;
//   * two people with one display name are told apart by a display-only suffix,
//     "Name (2)" (`displayRoster`), in chips, tooltips and accessible names;
//   * an avatar is drawn only from a non-null `avatarUrl`, through
//     `<img referrerpolicy="no-referrer">`, and falls back to initials on error.
//
// The remote caret label (`collab_presence.mjs`) reads the same seam, so a caret
// and its chip cannot disagree on a name or a colour.
//
// Complexity: everything here is O(participants), which the engine caps at
// `maxParticipants()`; nothing reads the document.
import { list as listFormat } from "./i18n.mjs";

/** Chips drawn before the rest fold into "+N" (Docs shows a handful, then a count). */
export const STRIP_CHIPS = 4;
/** Arrivals settle for this long after joining before any are announced, so the
 *  people already in the room are not read out as if they had just walked in. */
export const SETTLE_MS = 1500;
/** At most one join/leave announcement per window; changes inside it are batched. */
export const ANNOUNCE_MS = 2000;

/** The hue for relay participant `client`: the golden-angle palette the remote
 *  caret has always used, so a chip and a caret are the same colour. */
function hue(client) {
  return (Number(client) * 137.508) % 360;
}

/** `[r, g, b]` in 0..1 for `hsl(h 70% 38%)`, the palette's fixed saturation and lightness. */
function rgbOf(h, s = 0.7, l = 0.38) {
  const k = (n) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return [f(0), f(8), f(4)];
}

function luminance([r, g, b]) {
  const lin = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** The WCAG contrast ratio between two relative luminances. */
export function contrast(a, b) {
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** Black or white, whichever reads better on a background of luminance `l`.
 *  For any background one of the two clears 4.5:1. */
function inkFor(l) {
  return contrast(l, 1) >= contrast(l, 0) ? "#ffffff" : "#000000";
}

/** The participant's palette colour and the text colour that reads on it.
 *  White alone does not: on the palette's yellows it measures about 2.6:1. */
export function participantColor(client) {
  const rgb = rgbOf(hue(client));
  const background = luminance(rgb);
  return { color: `hsl(${hue(client)} 70% 38%)`, ink: inkFor(background), luminance: background };
}

/** A host's `#rrggbb` colour with its ink, or null for anything else (a colour
 *  this file cannot measure is not used, so its ink cannot be wrong). */
function hostColor(value) {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(String(value ?? ""));
  if (!m) return null;
  const rgb = m.slice(1).map((h) => parseInt(h, 16) / 255);
  return { color: `#${m.slice(1).join("")}`.toLowerCase(), ink: inkFor(luminance(rgb)) };
}

/** Whether an avatar may be drawn under the mount's policy (`docs/175` §8.1): a
 *  fetched image tells its server who looked and when, so only with consent; a
 *  `data:` URL of at most 1,024 characters fetches nothing and needs none. */
function avatarAllowed(url, avatars) {
  const value = String(url ?? "");
  if (!value) return false;
  return avatars === "fetch" || (value.startsWith("data:") && value.length <= 1024);
}

/** Initials from a display name: the first letter of its first and last words. */
function initialsOf(name) {
  const words = String(name).trim().split(/\s+/u).filter(Boolean);
  if (!words.length) return "?";
  const first = [...words[0]][0] ?? "";
  const last = words.length > 1 ? [...words[words.length - 1]][0] ?? "" : "";
  return (first + last).toLocaleUpperCase();
}

/** What one participant looks like in the chrome. THE seam (see the file comment).
 *
 *  Input (`docs/175` §14): `{ client, profile?, attested, isSelf }`. `client` is
 *  the relay's participant number (0-based, or null before the room has welcomed
 *  this tab); `profile` is `{ id, name, initials?, color?, avatarUrl? }` when a
 *  host supplies one; `attested` says the host vouches for it.
 *
 *  Output: `{ label, initials, color, avatarUrl, verified, isSelf }`, plus what
 *  the chrome needs to draw and address it: `key`, `client`, `ink`, and
 *  `numbered` (no profile: the label is the relay's number, and the chrome says
 *  so). `avatarUrl` is null unless the profile is attested — a claimed identity
 *  never shows a picture, because a picture is the easiest thing to borrow — AND
 *  the mount consented to fetching (`avatars: "fetch"`, `docs/175` §8.1), or the
 *  picture is a small `data:` URL that fetches nothing. `avatars` comes from the
 *  mount configuration the caller was built with, never from a global. */
export function participantDisplay(participant, { t, avatars = "never" }) {
  const client = participant?.client;
  const hasNumber = Number.isSafeInteger(client) && client >= 0;
  const profile = participant?.profile && String(participant.profile.name ?? "").trim() ? participant.profile : null;
  const verified = Boolean(profile && participant?.attested);
  const palette = participantColor(hasNumber ? client : 0);
  // A claimed colour could be anyone's; only a vouched-for one replaces the palette.
  const chosen = (verified && hostColor(profile.color)) || palette;
  const label = profile ? String(profile.name).trim()
    : hasNumber ? t("collab.participant", { number: client + 1 }) : "";
  const initials = profile
    ? String(profile.initials ?? "").trim() || initialsOf(label)
    : hasNumber ? t("collab.presence.initials", { number: client + 1 }) : "";
  return Object.freeze({
    key: hasNumber ? String(client) : "self",
    client: hasNumber ? client : null,
    isSelf: Boolean(participant?.isSelf),
    numbered: !profile,
    label, initials, verified,
    color: chosen.color, ink: chosen.ink,
    avatarUrl: verified && avatarAllowed(profile.avatarUrl, avatars) ? String(profile.avatarUrl) : null,
  });
}

/** Every participant's view, with duplicate labels told apart: the second and
 *  later holders of one label, in participant-number order, read "Name (2)",
 *  "Name (3)". Display only — nothing is renamed. O(participants). */
export function displayRoster(participants, { t, display = participantDisplay, avatars = "never" }) {
  const views = participants.map((p) => display(p, { t, avatars }));
  const seen = new Map();
  const order = views.map((v, i) => i).sort((a, b) => (views[a].client ?? -1) - (views[b].client ?? -1));
  const out = [...views];
  for (const i of order) {
    const view = views[i];
    if (view.numbered || !view.label) continue;
    const key = view.label.toLocaleLowerCase();
    const count = (seen.get(key) ?? 0) + 1;
    seen.set(key, count);
    if (count > 1) out[i] = Object.freeze({ ...view, label: t("collab.presence.duplicate", { name: view.label, count }) });
  }
  return out;
}

/** The accessible name, which is also the tooltip, for one chip. It says where a
 *  number stands in for a name, or where a name is only claimed, and what
 *  pressing the chip does. */
export function chipLabel(view, t) {
  const who = !view.label ? ""
    : view.numbered ? t("collab.presence.unnamed", { name: view.label })
      : view.verified ? view.label : t("collab.presence.unverified", { name: view.label });
  if (view.isSelf) return who ? t("collab.presence.you", { name: who }) : t("collab.presence.youAlone");
  return t("collab.presence.goTo", { name: who });
}

/** Normalises one roster entry: a bare participant number today, or an object
 *  carrying `profile`/`attested` once a host supplies them. */
function entry(value) {
  return typeof value === "object" && value !== null ? value : { client: value };
}

/** Self first, then everyone else in participant-number order (the engine's
 *  order, so two replicas draw the room alike); the first `max` are chips and the
 *  rest are the "+N". Entries are seam inputs (`participantDisplay`). */
export function planStrip({ self, others }, max = STRIP_CHIPS) {
  const mine = entry(self);
  const seen = new Set([mine.client]);
  const rest = others.map(entry).filter((p) => !seen.has(p.client) && seen.add(p.client))
    .sort((a, b) => a.client - b.client);
  const people = [{ ...mine, isSelf: true }, ...rest.map((p) => ({ ...p, isSelf: false }))];
  return { people, chips: people.slice(0, max), overflow: people.slice(max) };
}

/** Joins and leaves, said once per window rather than once per frame.
 *
 *  The baseline is the set of OTHER participants. `update` diffs against it;
 *  a join and a leave of one participant inside a window cancel out. `open`
 *  (and a reconnect) start a settle period in which the baseline is taken
 *  silently — everyone already there is not news. */
export function createJoinAnnouncer({ announce, name, setTimer = setTimeout, clearTimer = clearTimeout,
  settleMs = SETTLE_MS, windowMs = ANNOUNCE_MS, join = (items) => listFormat(items) }) {
  let baseline = new Set();
  let settling = false;
  let settleTimer = null;
  let flushTimer = null;
  const pending = new Map();
  function cancel() {
    if (settleTimer !== null) clearTimer(settleTimer);
    if (flushTimer !== null) clearTimer(flushTimer);
    settleTimer = flushTimer = null;
    pending.clear();
  }
  function flush() {
    flushTimer = null;
    const joined = [], left = [];
    for (const [client, change] of [...pending].sort((a, b) => a[0] - b[0])) (change === "joined" ? joined : left).push(name(client));
    pending.clear();
    const said = [joined.length ? ["joined", join(joined)] : null, left.length ? ["left", join(left)] : null].filter(Boolean);
    if (said.length) announce(said);
  }
  return {
    /** Starts (or restarts, on reconnect) a quiet settle period with an empty room. */
    open() {
      cancel();
      baseline = new Set();
      settling = true;
      settleTimer = setTimer(() => { settleTimer = null; settling = false; }, settleMs);
    },
    close() { cancel(); baseline = new Set(); settling = false; },
    update(others) {
      const next = new Set(others);
      if (!settling) {
        for (const client of next) if (!baseline.has(client)) {
          if (pending.get(client) === "left") pending.delete(client); else pending.set(client, "joined");
        }
        for (const client of baseline) if (!next.has(client)) {
          if (pending.get(client) === "joined") pending.delete(client); else pending.set(client, "left");
        }
        if (pending.size && flushTimer === null) flushTimer = setTimer(flush, windowMs);
      }
      baseline = next;
    },
  };
}


/** The strip itself. `parent` receives it; `goTo(client)` scrolls to that
 *  participant's caret and returns false when there is none to scroll to. */
export function createPresenceStrip({ root, parent, before = null, t, goTo, status,
  display = participantDisplay, avatars = "never", setTimer = setTimeout, clearTimer = clearTimeout }) {
  const strip = root.createElement("div");
  strip.id = "presenceStrip";
  strip.className = "presence-strip";
  strip.setAttribute("role", "group");
  strip.hidden = true;
  const chips = root.createElement("div");
  chips.className = "presence-chips";
  const more = root.createElement("button");
  more.type = "button";
  more.id = "presenceMore";
  more.className = "presence-chip presence-more";
  more.setAttribute("aria-haspopup", "true");
  more.setAttribute("aria-expanded", "false");
  more.setAttribute("aria-controls", "presenceList");
  // The phone face: one button with a count, ONLYOFFICE's mobile users button.
  const people = root.createElement("button");
  people.type = "button";
  people.id = "presencePeople";
  people.className = "btn presence-people";
  people.setAttribute("aria-haspopup", "true");
  people.setAttribute("aria-expanded", "false");
  people.setAttribute("aria-controls", "presenceList");
  const peopleIcon = root.createElement("span");
  peopleIcon.className = "ms";
  peopleIcon.setAttribute("aria-hidden", "true");
  peopleIcon.textContent = "group";
  const peopleCount = root.createElement("span");
  peopleCount.className = "presence-people-count";
  peopleCount.setAttribute("aria-hidden", "true");
  people.append(peopleIcon, peopleCount);
  const popup = root.createElement("div");
  popup.id = "presenceList";
  popup.className = "presence-list";
  popup.setAttribute("role", "group");
  popup.hidden = true;
  const heading = root.createElement("div");
  heading.id = "presenceListHeading";
  heading.className = "presence-list-heading";
  popup.setAttribute("aria-labelledby", heading.id);
  const rows = root.createElement("ul");
  rows.className = "presence-list-rows";
  popup.append(heading, rows);
  // Its own polite region, so a join never overwrites a refusal in the status line.
  const live = root.createElement("span");
  live.className = "sr-only";
  live.id = "presenceLive";
  live.setAttribute("role", "status");
  live.setAttribute("aria-live", "polite");
  strip.append(chips, more, people, popup, live);
  parent.insertBefore(strip, before);

  let state = { self: null, others: [] };
  /** The views last drawn, by participant key, so a press acts on what it showed. */
  let current = new Map();
  /** The last label each participant was drawn with, so a departure is announced
   *  by the name the reader saw, after the roster has dropped it. */
  const lastLabel = new Map();
  let shown = false;
  let opener = null;

  function initialsNode(view) {
    const text = root.createElement("span");
    text.setAttribute("aria-hidden", "true");
    text.textContent = view.initials;
    return text;
  }
  function chipFor(view, tag) {
    const chip = root.createElement(tag);
    chip.className = "presence-chip";
    if (tag === "button") chip.type = "button";
    paintChip(chip, view);
    return chip;
  }
  function paintChip(chip, view) {
    const label = chipLabel(view, t);
    chip.dataset.participant = view.key;
    chip.dataset.verified = String(view.verified);
    chip.classList.toggle("is-self", view.isSelf);
    // Claimed, not vouched for: drawn so it reads as a claim (`docs/175` §12).
    chip.classList.toggle("is-unverified", !view.numbered && !view.verified);
    chip.style.setProperty("--presence-color", view.color);
    chip.style.setProperty("--presence-ink", view.ink);
    chip.title = label;
    chip.setAttribute("aria-label", label);
    if (chip.tagName === "SPAN") chip.setAttribute("role", "img");
    const drawn = chip.firstElementChild;
    if (view.avatarUrl !== null) {
      if (drawn?.tagName === "IMG" && drawn.getAttribute("src") === view.avatarUrl) return;
      const img = root.createElement("img");
      img.alt = "";
      img.setAttribute("referrerpolicy", "no-referrer");
      img.decoding = "async";
      img.loading = "lazy";
      img.addEventListener("error", () => img.replaceWith(initialsNode(view)), { once: true });
      img.src = view.avatarUrl;
      chip.replaceChildren(img);
    } else if (drawn?.tagName !== "SPAN" || drawn.textContent !== view.initials) {
      chip.replaceChildren(initialsNode(view));
    }
  }
  /** Scrolls to a participant's caret, or says why it cannot. */
  function jump(view) {
    if (view.isSelf || view.client === null) return;
    if (!goTo(view.client)) status?.(t("collab.presence.noCaret", { name: view.label }), "warn");
  }

  function render() {
    if (!shown) return;
    const plan = planStrip(state);
    const everyone = displayRoster(plan.people, { t, display, avatars });
    current = new Map(everyone.map((view) => [view.key, view]));
    for (const view of everyone) if (view.client !== null) lastLabel.set(view.client, view.label);
    const chipViews = everyone.slice(0, plan.chips.length);
    // Chips are keyed by participant, so a re-render that changes nothing about
    // a chip leaves its element — and the keyboard focus on it — where it was.
    const existing = new Map([...chips.children].map((el) => [el.dataset.participant, el]));
    const ordered = chipViews.map((view) => {
      const tag = view.isSelf ? "SPAN" : "BUTTON";
      const reuse = existing.get(view.key);
      if (reuse && reuse.tagName === tag) { paintChip(reuse, view); return reuse; }
      const chip = chipFor(view, tag.toLowerCase());
      if (!view.isSelf) chip.addEventListener("click", () => jump(current.get(chip.dataset.participant) ?? view));
      return chip;
    });
    if (ordered.length !== chips.children.length || ordered.some((el, i) => chips.children[i] !== el)) chips.replaceChildren(...ordered);

    const total = plan.people.length;
    strip.setAttribute("aria-label", t("collab.presence.label"));
    more.hidden = plan.overflow.length === 0;
    more.textContent = t("collab.presence.more", { count: plan.overflow.length });
    more.title = t("collab.presence.moreLabel", { count: plan.overflow.length });
    more.setAttribute("aria-label", more.title);
    peopleCount.textContent = String(total);
    people.title = t("collab.presence.everyone", { count: total });
    people.setAttribute("aria-label", people.title);
    heading.textContent = people.title;
    if (!popup.hidden) renderRows(everyone);
  }
  function renderRows(everyone) {
    rows.replaceChildren(...everyone.map((view) => {
      const li = root.createElement("li");
      const row = root.createElement(view.isSelf ? "div" : "button");
      row.className = "presence-row";
      if (!view.isSelf) {
        row.type = "button";
        row.dataset.participant = view.key;
        row.addEventListener("click", () => { close(); jump(view); });
      }
      const chip = chipFor(view, "span");
      // The row's text names the person; the chip inside it is decoration there.
      chip.removeAttribute("role"); chip.removeAttribute("aria-label"); chip.removeAttribute("title");
      chip.setAttribute("aria-hidden", "true");
      const label = root.createElement("span");
      label.className = "presence-row-label";
      label.textContent = chipLabel(view, t);
      row.append(chip, label);
      li.append(row);
      return li;
    }));
  }
  function open(from) {
    opener = from;
    popup.hidden = false;
    render();
    from.setAttribute("aria-expanded", "true");
    (rows.querySelector("button") ?? from).focus();
  }
  function close({ restore = true } = {}) {
    if (popup.hidden) return;
    popup.hidden = true;
    more.setAttribute("aria-expanded", "false");
    people.setAttribute("aria-expanded", "false");
    if (restore) opener?.focus();
    opener = null;
  }
  for (const trigger of [more, people]) {
    trigger.addEventListener("click", () => (popup.hidden ? open(trigger) : close()));
  }
  popup.addEventListener("keydown", (event) => {
    const items = [...rows.querySelectorAll("button")];
    const at = items.indexOf(root.activeElement);
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
    else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (items.length) items[(at + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length].focus();
    } else if (event.key === "Tab") close({ restore: false });
  });
  root.addEventListener("pointerdown", (event) => {
    if (!popup.hidden && !strip.contains(event.target)) close({ restore: false });
  });

  const announcer = createJoinAnnouncer({
    setTimer, clearTimer,
    name: (client) => lastLabel.get(client) ?? display({ client }, { t }).label,
    announce: (said) => {
      live.textContent = said.map(([change, names]) => (change === "joined"
        ? t("collab.presence.joined", { names }) : t("collab.presence.left", { names }))).join(" ");
    },
  });
  const clientOf = (value) => (typeof value === "object" && value !== null ? value.client : value);

  return {
    element: strip,
    /** Shows the strip for a document in a room, or withdraws it. */
    show(inRoom) {
      if (inRoom === shown) return;
      shown = inRoom;
      strip.hidden = !inRoom;
      close({ restore: false });
      if (inRoom) { announcer.open(); render(); } else { announcer.close(); live.textContent = ""; lastLabel.clear(); }
    },
    /** A new roster. `self` is this tab's participant, `others` everyone else's;
     *  each is a participant number today, or a seam input once profiles exist. */
    update({ self = null, others = [], reconnected = false }) {
      const mine = clientOf(self);
      const next = { self, others: others.filter((p) => clientOf(p) !== mine) };
      // Called on every frame, document frames included: an unchanged room of
      // bare numbers costs a comparison, not a repaint.
      const same = mine === clientOf(state.self) && next.others.length === state.others.length
        && next.others.every((p, i) => p === state.others[i]);
      state = next;
      if (!shown || (same && !reconnected)) return;
      if (reconnected) announcer.open();
      announcer.update(state.others.map(clientOf));
      render();
    },
    /** Re-reads every label, for a language that arrived or changed. */
    reflect: render,
  };
}
