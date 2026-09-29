// Region composition: the owner's container policy, as properties.
//
// `docs/126`'s container policy makes three demands that this file exists to hold.
//
//   1. Surface composition is a DIFFERENT question from command gating. A role
//      with no business with a whole surface does not get the surface; within a
//      surface it does get, a command that cannot run right now is disabled and
//      says why. "Never, for you" versus "not right now".
//   2. `preview` and `readonly` are NOT the same thing, and must not be collapsed.
//   3. Policies are per capability, not per tier. Roles stay as presets over the
//      capability set, never the unit of enforcement, and both must resolve
//      through the SAME code.
//
// The third is the one with a track record: two code paths for roles and explicit
// lists "will diverge exactly as the `shortcut:` labels and key bindings did"
// (`109` UX-006/UX-007). So the assertions below are not about the tables' shape.
// They are about properties that hold for a role added later, and several of them
// would fail on a second resolver even if both resolvers were individually correct.
//
// The DOM half is `chrome_regions.test.mjs` (which elements each region names) and
// `tests/e2e/white-label.spec.mjs` (that a real editor composes them away).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  CAPABILITIES,
  CAPABILITY_AFFORDANCES,
  EDITING_REGIONS,
  PRESET_NAMES,
  REGIONS,
  ROLES,
  editingModeFor,
  hostChrome,
  hostConfig,
  hostRegions,
  reflectReviewModeAccess,
  parseWithheld,
  resolveCapabilities,
  resolveRegions,
} from "../src/capabilities.mjs";

const caps = (input) => [...resolveCapabilities(input)].sort();
const regions = (input) => [...resolveRegions(input)].sort();

test("every preset resolves both axes, and only to names that exist", () => {
  for (const mode of PRESET_NAMES) {
    for (const id of resolveRegions({ mode })) {
      assert.ok(REGIONS.includes(id), `${mode} resolves to an unknown region "${id}"`);
    }
    for (const name of resolveCapabilities({ mode })) {
      assert.ok(CAPABILITIES.includes(name), `${mode} resolves to an unknown capability "${name}"`);
    }
  }
});

test("a readonly container has NO editing ribbon — not a dimmed one", () => {
  // The owner's sentence, verbatim: "A `readonly` container has no editing ribbon.
  // Not a ribbon full of greyed buttons — no ribbon." Word, Google Docs and
  // ONLYOFFICE all present read-only as a different chrome rather than the editing
  // chrome dimmed, and a wall of greyed controls tells a reader about capabilities
  // they will never have while burying the one or two things they can do.
  const shown = resolveRegions({ mode: "readonly", framed: true });
  assert.equal(shown.has("ribbon"), false, "readonly must not get the ribbon");
  for (const id of REGIONS.filter((r) => r.startsWith("band."))) {
    assert.equal(shown.has(id), false, `readonly must not get ${id}`);
  }
  // And it IS a reading experience, not a bare canvas: the reader navigates,
  // searches, sees where they are, and can take a paper copy.
  for (const id of ["menu", "rail", "status", "zoom", "find", "title"]) {
    assert.equal(shown.has(id), true, `readonly must get ${id} to be readable`);
  }
  // The selection toolbar offers formatting, and Settings is an author's
  // preferences; neither belongs to a reader.
  assert.equal(shown.has("selection"), false);
  assert.equal(shown.has("settings"), false);
});

test("preview and readonly are different presentations, not one with a capability removed", () => {
  // `docs/126` is explicit: "preview is not `readonly` minus print. Collapsing
  // them would give every attachment preview a reading UI it does not want, or
  // every published document a bare canvas with no way to get to page 40."
  const preview = resolveRegions({ mode: "preview", framed: true });
  const readonly = resolveRegions({ mode: "readonly", framed: true });
  assert.deepEqual([...preview], [], "preview is the runtime as a rendering engine");
  assert.ok(readonly.size >= 6, "readonly is a reading experience and needs chrome for it");
  // The difference between the two is bigger than one capability — which is the
  // whole claim. If it were only `print`, a future edit could collapse them and
  // nothing would notice.
  const capDelta = caps({ mode: "readonly" }).filter((c) => !caps({ mode: "preview" }).includes(c));
  assert.deepEqual(capDelta, ["print"]);
  assert.ok(
    readonly.size - preview.size > capDelta.length,
    "preview and readonly differ by one capability and no chrome, so they have been collapsed",
  );
  // Both are still readers: neither may change, keep or replace the document.
  for (const mode of ["preview", "readonly"]) {
    assert.equal(editingModeFor(resolveCapabilities({ mode })), "viewing");
  }
});

test("every capability a preset GRANTS has an affordance in chrome that preset SHOWS", () => {
  // THE JOINT BETWEEN THE TWO AXES, and the assertion that shaped reading chrome.
  // Composition removes surfaces and a permission grants an action; nothing
  // mechanical otherwise stops a container being handed a capability and no way to
  // use it. That is `105` UX-004's recurring defect ("capability reachable from
  // only ONE surface") arriving one level up, and it is exactly how hiding the
  // ribbon for `readonly` WITHOUT revealing the menu bar would have made its only
  // grant unreachable.
  const unreachable = [];
  for (const mode of PRESET_NAMES) {
    const shown = resolveRegions({ mode, framed: true });
    for (const name of resolveCapabilities({ mode, framed: true })) {
      const where = CAPABILITY_AFFORDANCES[name];
      assert.ok(where, `${name} has no declared affordance, so this guard cannot check it`);
      if (where.length === 0) continue; // needs no chrome; the table says why
      if (!where.some((id) => shown.has(id))) unreachable.push(`${mode} grants ${name} but shows none of ${where.join(", ")}`);
    }
  }
  assert.deepEqual(unreachable, []);
  // The half that fails when the guard breaks rather than when the tree does: a
  // table of empty lists would pass the loop above having checked nothing.
  const checkable = Object.values(CAPABILITY_AFFORDANCES).filter((l) => l.length > 0);
  assert.ok(checkable.length >= 6, "too few capabilities have a declared affordance to check");
  assert.deepEqual(
    Object.keys(CAPABILITY_AFFORDANCES).sort(),
    [...CAPABILITIES].sort(),
    "every capability must appear in the affordance table, or a new one is exempt by omission",
  );
});

test("roles and explicit capability lists resolve through ONE authority", () => {
  // THE PROPERTY THAT PROVES IT, rather than a comment claiming it. Every role's
  // capability set is reachable by narrowing the role above it with a withhold
  // list, through the same `resolveCapabilities`. If roles were a parallel
  // mechanism — a second table, or a branch on a role name — the sets would not
  // line up, because nothing would be forcing them to.
  for (let i = 1; i < ROLES.length; i += 1) {
    const lower = resolveCapabilities({ mode: ROLES[i - 1] });
    const higher = resolveCapabilities({ mode: ROLES[i] });
    const extra = [...higher].filter((name) => !lower.has(name));
    const narrowed = resolveCapabilities({ mode: ROLES[i], withhold: extra.map((n) => `-${n}`).join(",") });
    assert.deepEqual(
      [...narrowed].sort(),
      [...lower].sort(),
      `${ROLES[i]} narrowed by ${extra.join(",")} is not ${ROLES[i - 1]}, so roles and explicit ` +
        "lists are not one mechanism",
    );
  }
  // And the per-capability policy the container notes ask for is real: "comments
  // only, everything else off" is expressible, not a rung on a ladder.
  assert.deepEqual(
    caps({ mode: "commentor", withhold: "-print,-download" }),
    ["comment"],
    "a host must be able to withhold print and download from a commentor independently",
  );
  // Each of the five the owner named, withheld one at a time, from a role that has it.
  for (const name of ["print", "download", "save", "edit", "comment"]) {
    const full = resolveCapabilities({ mode: "owner" });
    assert.ok(full.has(name), `owner does not grant ${name}, so this case checks nothing`);
    const narrowed = resolveCapabilities({ mode: "owner", withhold: `-${name}` });
    assert.equal(narrowed.has(name), false, `${name} is not independently withholdable`);
    assert.equal(narrowed.size, full.size - 1, `withholding ${name} moved something else`);
  }
});

test("a withhold list can only ever take away", () => {
  // One direction to audit. A configuration channel that can widen a role is a
  // configuration channel an attacker fills in, and `docs/125` §5.2 states the rule:
  // "`can` may only narrow the role. A host cannot grant `edit` to `preview` by
  // accident."
  for (const spelling of ["edit", "-edit", "+edit", "EDIT", " edit ", "edit,edit"]) {
    assert.equal(
      resolveCapabilities({ mode: "preview", withhold: spelling }).size,
      0,
      `"${spelling}" changed what preview grants`,
    );
  }
  assert.equal(resolveRegions({ mode: "preview", withhold: "ribbon,-menu" }).size, 0);
  // An unknown entry is DROPPED, and dropping narrows nothing — so a typo in a
  // host's URL can never widen the result. Same reasoning as an unrecognised
  // `mode` falling back to the framed default.
  assert.deepEqual([...parseWithheld("-nonsense,-print", CAPABILITIES)], ["print"]);
  assert.deepEqual([...parseWithheld(null, CAPABILITIES)], []);
  assert.deepEqual([...parseWithheld("", REGIONS)], []);
  // And it really does narrow when the name is known, or the assertions above
  // would pass for a parser that ignored everything.
  assert.equal(resolveCapabilities({ mode: "owner", withhold: "-edit" }).has("edit"), false);
});

test("withholding the ribbon withholds its bands, derived rather than asked twice", () => {
  const shown = resolveRegions({ mode: "owner", withhold: "-ribbon" });
  assert.equal(shown.has("ribbon"), false);
  for (const id of REGIONS.filter((r) => r.startsWith("band."))) {
    assert.equal(shown.has(id), false, `${id} survived its container being withheld`);
  }
  // A host who said `-ribbon` and still saw a band would have found a hole; a host
  // who had to list all eight would be maintaining our containment rules for us.
  assert.ok(shown.has("status"), "withholding the ribbon must not take the status bar with it");
  // And one band alone is expressible, which is the grain a host actually asks for.
  const homeOnly = resolveRegions({ mode: "owner", withhold: "-band.insert,-band.layout,-band.references,-band.review,-band.view,-band.table" });
  assert.equal(homeOnly.has("band.home"), true);
  assert.equal(homeOnly.has("band.insert"), false);
  assert.equal(homeOnly.has("ribbon"), true);
});

test("a framed container with no mode asked gets the framed presentation, never standalone", () => {
  // The phase 1 default, now on both axes. A page that is not the top window was
  // put there by someone else.
  assert.deepEqual(regions({ framed: true }), regions({ mode: "embedded" }));
  assert.deepEqual(caps({ framed: true }), caps({ mode: "embedded" }));
  // An unrecognised mode falls back to the framed default rather than throwing,
  // and never to more than it: a typo must not leave a container with no chrome at
  // all, and must not grant more either.
  assert.deepEqual(regions({ mode: "kittens", framed: true }), regions({ mode: "embedded" }));
  assert.deepEqual(caps({ mode: "kittens", framed: true }), caps({ mode: "embedded" }));
});

test("the URL is the one configuration channel, read in one place", () => {
  // Every host input travels on the URL, because that is the only channel decided
  // BEFORE the frame's first navigation (`embed_element.mjs` writes the mode into
  // the src). A `postMessage` after load would mean a window in which the editor
  // was something else, and `docs/104` is explicit that gating applies before the
  // first frame.
  const view = {
    location: {
      search: "?mode=readonly&can=-print&chrome=-find&autosave=1&prefs=%7B%22spellcheck%22%3Afalse%7D",
    },
    self: 1,
    top: 2,
  };
  const config = hostConfig(view);
  assert.deepEqual(config, {
    mode: "readonly",
    autosave: true,
    withhold: "-print",
    chrome: "-find",
    // The opening positions, read in the same place and at the same moment as
    // the other four. Two places that know how a host configures this editor is
    // how `autosave` came to be known by both this file and `main.js`.
    prefs: '{"spellcheck":false}',
    framed: true,
  });
  assert.deepEqual([...hostRegions(view)].sort(), regions({ mode: "readonly", framed: true, withhold: "-find" }));
  // A cross-origin parent makes `self !== top` throw in some engines; a throw means
  // we ARE framed, which is the safer answer.
  const hostile = { location: { search: "" }, get self() { throw new Error("cross-origin"); } };
  assert.equal(hostConfig(hostile).framed, true);
});

// ── The reading chrome, for a document that cannot be edited ──────────────────

test("a document that cannot be edited loses the editing chrome and NOTHING else", () => {
  // The owner's report: previewing a stored version showed the whole editing ribbon
  // over a document you cannot edit. That is the same "never, for you" `docs/126`
  // draws — arriving from the DOCUMENT rather than from the host — so the honest
  // answer is the same one `readonly` already gets: no editing ribbon, not a ribbon
  // of greyed bands.
  const { editing, reading } = hostChrome({ location: { search: "" }, self: 1, top: 1 });
  assert.equal(editing.has("ribbon"), true, "our own page has the ribbon to begin with");
  for (const id of EDITING_REGIONS) {
    assert.equal(reading.has(id), false, `${id} is editing chrome and must be composed away`);
  }
  // And everything else survives, named rather than counted. The rail in particular:
  // it carries the Versions entry, so a preview that took it away would strand the
  // reader in the preview with no way back to the timeline they came from.
  for (const id of REGIONS.filter((r) => !EDITING_REGIONS.includes(r))) {
    assert.equal(reading.has(id), true, `${id} is not editing chrome and must survive`);
  }
  assert.equal(reading.has("rail"), true, "the way back to the timeline");
  assert.equal(reading.has("history"), true, "the panel and its preview bar live here");
  assert.equal(reading.has("menu"), true, "one navigation axis, which the ribbon's absence reveals");
  assert.equal(reading.has("settings"), true, "preferences are not document edits");
  assert.equal(reading.has("brand"), true, "it is still our own page");
});

test("the reading chrome can only narrow what a host composed, never widen it", () => {
  // Structural, not incidental: `reading` is produced by APPENDING the editing
  // regions to the host's own `?chrome=` withhold list, and `parseWithheld` runs in
  // one direction. So a mode cannot hand back a surface the host took away — which
  // is the whole reason composition and permission are separate axes.
  for (const search of [
    "",
    "?mode=readonly",
    "?mode=preview",
    "?mode=viewer",
    "?mode=commentor",
    "?mode=embedded",
    "?chrome=-rail,-find,-status",
    "?mode=owner&can=-branding",
  ]) {
    const { editing, reading } = hostChrome({ location: { search }, self: 1, top: 2 });
    for (const id of reading) {
      assert.ok(editing.has(id), `"${search}" let the reading chrome add "${id}"`);
    }
  }
  // A host that withheld the rail does not get it back for a preview.
  const narrowed = hostChrome({ location: { search: "?chrome=-rail" }, self: 1, top: 1 });
  assert.equal(narrowed.reading.has("rail"), false);
  // And a container that already had no ribbon is unchanged by the distinction —
  // which is what says the two answers agree wherever the host has said anything.
  const reader = hostChrome({ location: { search: "?mode=readonly" }, self: 1, top: 2 });
  assert.deepEqual([...reader.reading].sort(), [...reader.editing].sort());
  const preview = hostChrome({ location: { search: "?mode=preview" }, self: 1, top: 2 });
  assert.equal(preview.editing.size, 0);
  assert.equal(preview.reading.size, 0);
});

test("`hostRegions` and `hostChrome().editing` are one answer, not two", () => {
  // `hostRegions` used to read the URL and resolve on its own. Two functions reading
  // the same inputs are two answers waiting to disagree — the defect this file's own
  // comments record for the `shortcut:` labels and the key bindings — so it delegates.
  for (const search of ["", "?mode=readonly", "?chrome=-ribbon", "?mode=edit&can=-print"]) {
    const view = { location: { search }, self: 1, top: 2 };
    assert.deepEqual([...hostRegions(view)].sort(), [...hostChrome(view).editing].sort());
  }
});

test("`branding` is consulted: an embedded editor does not advertise us", () => {
  // It was declared in phase 1 and consulted NOWHERE, with a test asserting so and
  // the embedding page saying in prose that withholding it does nothing. A capability
  // that does nothing is a dead control at the API level — the same defect `SKILL` §10
  // forbids in the chrome.
  //
  // It is wired to the `brand` region, because that is what it always meant: the
  // `edit` preset's own comment says an embedded editor "does not advertise us inside
  // their product". So the default for a host embedding the editor is no mark of ours,
  // without them having to discover a parameter.
  for (const mode of ["owner", "standalone"]) {
    assert.equal(resolveCapabilities({ mode }).has("branding"), true);
    assert.equal(resolveRegions({ mode }).has("brand"), true, `${mode} is our own page`);
  }
  for (const mode of ["edit", "embedded", "commentor", "readonly", "preview", "viewer"]) {
    assert.equal(resolveCapabilities({ mode }).has("branding"), false);
    assert.equal(
      resolveRegions({ mode, framed: true }).has("brand"),
      false,
      `${mode} shows our mark inside someone else's product`,
    );
  }
  // And a `?can=` list reaches it, which is what proves the wiring reads the RESOLVED
  // grant rather than the preset name.
  assert.equal(
    resolveRegions({
      mode: "owner",
      capabilities: resolveCapabilities({ mode: "owner", withhold: "-branding" }),
    }).has("brand"),
    false,
    "`?can=-branding` did not reach the brand region",
  );
});

// ── The Viewing banner's "Switch to editing" offer ────────────────────────────

test("the banner's editing offer is withdrawn wherever it could not be taken", () => {
  // `reflectReviewModeAccess` disables a withheld MODE and says why, because a
  // segment still tells the reader which of three modes they are in. The banner's
  // offer is different in kind: it is an invitation, and an invitation that cannot
  // be accepted is a dead control however politely it is greyed. It was withdrawn
  // only for `readOnlyReason` — so a `preview` container, whose host withheld
  // `edit` outright, offered to switch into an editing mode it can never enter.
  const reflect = (mode, readOnlyReason = "") => {
    const bannerEdit = { hidden: false };
    const buttons = ["editing", "suggesting", "viewing"].map((name) => ({
      dataset: { reviewMode: name },
      disabled: false,
      title: "",
      removeAttribute() {
        this.title = "";
      },
    }));
    reflectReviewModeAccess({
      buttons,
      bannerEdit,
      capabilities: resolveCapabilities({ mode, framed: true }),
      readOnlyReason,
      withheldReason: "withheld",
    });
    return { bannerEdit, buttons };
  };

  for (const mode of ["preview", "readonly", "viewer"]) {
    assert.equal(reflect(mode).bannerEdit.hidden, true, `${mode} cannot switch to editing`);
  }
  for (const mode of ["edit", "owner", "standalone"]) {
    assert.equal(reflect(mode).bannerEdit.hidden, false, `${mode} can, so the offer stands`);
  }
  // And the engine's refusal still outranks everything: the document itself cannot
  // be edited, whatever the host granted.
  assert.equal(reflect("owner", "This document is too large to edit").bannerEdit.hidden, true);
  // The SEGMENTS are not removed in any of those cases — they are disabled with a
  // reason, which is the distinction this test exists to hold apart.
  const { buttons } = reflect("readonly");
  assert.equal(buttons[0].disabled, true);
  assert.equal(buttons[0].title, "withheld");
});
