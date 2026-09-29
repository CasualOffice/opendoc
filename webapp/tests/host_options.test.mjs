// The competitor's configuration, and what we do with it.
//
// `host_options.mjs` is a translation table between two vocabularies, and a
// translation table has exactly two ways to be wrong: it can name a word the
// target language does not have, and it can translate in the wrong direction.
// Both are checked here, because neither is visible at a call site — a row whose
// `via` names a capability that does not exist translates to nothing at all and
// looks like a host who configured nothing.
//
// THE THIRD THING, and the reason this file is not only a table check: an option
// that "maps" must actually CHANGE THE CONTAINER. Every assertion below that
// claims a mapping runs the translation through `resolveCapabilities` and
// `resolveRegions` — the authority, the same call the editor makes — and asserts
// the resolved set really lost the thing. A guard that stopped at "the table has
// a row for it" would pass while the row did nothing, which is the shape of
// every green-but-wrong test `SKILL` §4 lists.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CAPABILITIES,
  PREFERENCES,
  REGIONS,
  resolveCapabilities,
  resolvePreferences,
  resolveRegions,
} from "../src/capabilities.mjs";
import { PRODUCT_DEFAULTS } from "../src/settings_defaults.mjs";
import {
  NOTE_CODES,
  OPTIONS,
  OPTION_PATHS,
  optionTally,
  translateHostConfig,
  unhandledNotes,
  unknownTargets,
} from "../src/host_options.mjs";

/** The three axes, resolved the way the editor resolves them. */
const containerFor = (config) => {
  const { mode, can, chrome, prefs } = translateHostConfig(config);
  const capabilities = resolveCapabilities({
    mode,
    framed: true,
    withhold: can.map((name) => `-${name}`).join(","),
  });
  return {
    capabilities,
    regions: resolveRegions({
      mode,
      framed: true,
      withhold: chrome.map((id) => `-${id}`).join(","),
      capabilities,
    }),
    settings: resolvePreferences(prefs),
  };
};

const codeOf = (notes, option) => notes.find((note) => note.option === option)?.code;

// ---- The table itself -------------------------------------------------------

test("every mapping names something the authority actually has", () => {
  // The failure this exists for: a row reading `cap("printing")` translates to
  // nothing, and a host who wrote `permissions.print: false` gets a container
  // that can still print — with no error anywhere, because dropping an unknown
  // name is the SAFE behaviour everywhere else in this model.
  assert.deepEqual(
    unknownTargets(),
    [],
    "a mapping that names nothing translates to nothing, and looks exactly like a " +
      "host who configured nothing",
  );
});

test("the table states no vocabulary of its own", () => {
  // The whole constraint on this module: `capabilities.mjs` stays the single
  // authority, and nothing here may be a second list of what a capability or a
  // region is. Asserted by construction — every emitted name is one of theirs.
  const emitted = OPTIONS.filter((row) => row.via);
  assert.ok(emitted.length > 20, "the table has stopped translating anything");
  for (const row of emitted) {
    const known =
      row.via.kind === "capability"
        ? CAPABILITIES
        : row.via.kind === "region"
          ? REGIONS
          : PREFERENCES.map((pref) => pref.name);
    assert.ok(known.includes(row.via.target), `${row.option} names an unknown ${row.via.kind}`);
  }
});

test("every option is answered exactly once, and every answer is a known code", () => {
  assert.equal(new Set(OPTION_PATHS).size, OPTION_PATHS.length, "an option is listed twice");
  for (const row of OPTIONS) {
    assert.ok(NOTE_CODES.includes(row.code), `${row.option} carries the code "${row.code}"`);
    // A row that is not a plain success owes the host a sentence. "Unsupported"
    // on its own is the shrug this table exists to replace: the note is what
    // turns it into a row somebody can act on.
    if (row.code !== "mapped") {
      assert.ok(row.note || row.ours, `${row.option} is "${row.code}" and says nothing about why`);
    }
  }
  // Derived, never written down — the counts the published table quotes.
  const tally = optionTally();
  assert.equal(
    Object.values(tally).reduce((a, b) => a + b, 0),
    OPTIONS.length,
    "the tally and the table disagree",
  );
});

test("nothing in the table is gated, tiered, or metered", () => {
  // The product argument, as a guard. ONLYOFFICE's equivalent of this module is
  // `LayoutManager._applyCustomization`, and its second line is
  // `if (!_licensed || !config) return;`. Ours must never acquire one — the
  // permissive licence is the wedge, and gating this would surrender it.
  const source = OPTIONS.map((row) => `${row.ours ?? ""} ${row.note ?? ""}`).join("\n");
  for (const word of [/\blicen[cs]ed\b/i, /\bpaid\b/i, /\btier\b/i, /\bper-tab\b/i]) {
    assert.doesNotMatch(source, word, "a licence condition has appeared in the option map");
  }
});

// ---- Direction: only `false` narrows ----------------------------------------

test("a permission set to false really takes the capability away", () => {
  const { capabilities } = containerFor({ permissions: { print: false, download: false } });
  assert.equal(capabilities.has("print"), false);
  assert.equal(capabilities.has("download"), false);
  // And the rest of the role is untouched: a withhold list narrows, it does not
  // replace.
  assert.equal(capabilities.has("edit"), true);
});

test("a permission set to true cannot widen the role, and says where to ask", () => {
  // The property the whole model rests on. Their booleans go both ways; ours go
  // one, because a configuration channel that can widen a role is a channel an
  // attacker fills in. `readonly` grants `print` and nothing else, so this asks
  // for `edit` in the one way their config allows and must not get it.
  const { capabilities } = containerFor({
    editorConfig: { mode: "view" },
    permissions: { edit: true, comment: true },
  });
  assert.equal(capabilities.has("edit"), false);
  assert.equal(capabilities.has("comment"), false);
  const { notes } = translateHostConfig({ permissions: { edit: true } });
  assert.equal(codeOf(notes, "permissions.edit"), "cannot-widen");
  assert.match(notes[0].message, /mode/, "the refusal must name where the grant can be asked for");
});

test("an inverted option is read in the direction its name means", () => {
  // `hideRulers: true` HIDES. Getting this backwards would turn a host's "show
  // the rulers" into "hide them", which is the class of bug a boolean table
  // exists to make impossible to write twice — so both directions are asserted.
  const hidden = containerFor({ editorConfig: { customization: { hideRulers: true } } });
  assert.equal(hidden.regions.has("ruler"), false);
  const shown = containerFor({ editorConfig: { customization: { hideRulers: false } } });
  assert.equal(shown.regions.has("ruler"), true);
});

test("their toolbar tabs become our bands, and the ribbon takes its bands with it", () => {
  const { regions } = containerFor({
    editorConfig: { customization: { layout: { toolbar: { home: false, collaboration: false } } } },
  });
  assert.equal(regions.has("band.home"), false);
  // Their Collaboration tab is our Review band, which is the one row in the
  // toolbar group that is a rename rather than a match.
  assert.equal(regions.has("band.review"), false);
  assert.equal(regions.has("band.insert"), true);
  // And the containment rule is the authority's, not re-implemented here.
  const whole = containerFor({ editorConfig: { customization: { layout: { toolbar: false } } } });
  assert.equal(whole.regions.has("ribbon"), false);
  assert.equal(whole.regions.has("band.home"), false);
});

test("the review-mode switcher can be withheld without touching the grant", () => {
  // ONLYOFFICE `customization.review.hideReviewDisplay`. Composition and
  // permission are separate axes, and this is the test of that: the control is
  // gone and every capability the role grants is still granted.
  const { regions, capabilities } = containerFor({
    editorConfig: { customization: { review: { hideReviewDisplay: true } } },
  });
  assert.equal(regions.has("review"), false);
  assert.equal(capabilities.has("edit"), true);
  assert.equal(capabilities.has("comment"), true);
});

// ---- Preferences ------------------------------------------------------------

test("every preference lands on a setting the editor really has", () => {
  // A preference with nowhere to land is a dead control in configuration form:
  // the host sets it, the container reports it applied, and nothing changes.
  for (const row of PREFERENCES) {
    assert.ok(
      Object.hasOwn(PRODUCT_DEFAULTS, row.setting),
      `the "${row.name}" preference writes ${row.setting}, which is not a setting`,
    );
  }
});

test("a host's opening positions reach the settings the editor merges", () => {
  const { settings } = containerFor({
    editorConfig: {
      user: { name: "Ada Lovelace" },
      customization: { spellcheck: false, uiTheme: "dark" },
    },
  });
  assert.equal(settings.authorName, "Ada Lovelace");
  assert.equal(settings.spellCheck, false);
  assert.equal(settings.theme, "dark");
});

test("a value the preference cannot mean is dropped, not coerced", () => {
  // `spellcheck: "yes"` is a host mistake. Coercing it to `true` would mean the
  // container silently disagreed with the configuration the host is reading.
  assert.deepEqual(resolvePreferences({ spellcheck: "yes" }), {});
  assert.deepEqual(resolvePreferences({ theme: "chartreuse" }), {});
  assert.deepEqual(resolvePreferences({ notAPreference: 1 }), {});
  // And a comma inside a value survives, which is why this channel is JSON and
  // not the comma list `can` and `chrome` use.
  assert.deepEqual(resolvePreferences('{"user":"Lovelace, Ada"}'), { authorName: "Lovelace, Ada" });
  // Malformed text leaves the editor with its own defaults rather than no editor.
  assert.deepEqual(resolvePreferences("{not json"), {});
});

// ---- Reporting --------------------------------------------------------------

test("nothing a host configures is silently ignored", () => {
  // The claim this module is for. Every leaf of their config produces a note,
  // including the ones we decline and the ones they misspelled — which is the
  // half `_applyCustomization` cannot do, because an early return has nothing
  // to say.
  const config = {
    permissions: { print: false, chat: true },
    editorConfig: {
      lang: "fr",
      customization: { macros: true, unit: "cm", nonsense: 1, layout: { rightMenu: false } },
    },
  };
  const { notes } = translateHostConfig(config);
  const answered = new Set(notes.map((note) => note.option));
  assert.deepEqual(
    [...answered].sort(),
    [
      "customization.layout.rightMenu",
      "customization.macros",
      "customization.nonsense",
      "customization.unit",
      "editorConfig.lang",
      "permissions.chat",
      "permissions.print",
    ],
    "an option a host wrote produced no answer at all",
  );
  assert.equal(codeOf(notes, "customization.macros"), "declined");
  assert.equal(codeOf(notes, "customization.unit"), "unsupported");
  assert.equal(codeOf(notes, "customization.nonsense"), "unknown");
  // Every note carries something a person can act on.
  for (const note of notes) assert.ok(note.message.length > 0, `${note.option} has an empty note`);
  // And the console only hears what did not simply work.
  const unhandled = unhandledNotes(notes).map((note) => note.option);
  assert.equal(unhandled.includes("permissions.print"), false);
  assert.equal(unhandled.includes("customization.macros"), true);
});

test("an empty or unreadable config changes nothing and says nothing", () => {
  for (const input of [null, undefined, "", "{broken", 7, []]) {
    const result = translateHostConfig(input);
    assert.equal(result.mode, null);
    assert.deepEqual([...result.can], []);
    assert.deepEqual([...result.chrome], []);
    assert.deepEqual([...result.notes], []);
  }
});

test("a config that points at itself terminates", () => {
  // Their `_applyCustomization` recurses over the same shape and would not.
  const cyclic = { editorConfig: {} };
  cyclic.editorConfig.customization = cyclic;
  assert.doesNotThrow(() => translateHostConfig(cyclic));
});
