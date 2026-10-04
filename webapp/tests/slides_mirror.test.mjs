// The deck's off-screen structural mirror (docs/156 Tier 3).
//
// A rendered slide is pixels, so this module is the ONLY thing on the slides page
// besides the sorter that a screen reader can read. What is worth asserting is
// therefore the structure itself — a title is a heading, depth is nesting, the
// deck's repeated furniture is its own region — and each of those is a thing a
// reader hears or does not hear.
//
// No DOM library: the repository has none, and the handful of element operations
// this module performs stand up by hand in forty lines. That also keeps the test
// honest about exactly which DOM surface the mirror depends on — if it starts
// needing more, this stub fails rather than a browser silently covering for it.
import assert from "node:assert/strict";
import test from "node:test";

/** One element, with just enough of the DOM for the mirror to build into. */
function element(tag) {
  const node = {
    tagName: tag.toUpperCase(),
    children: [],
    attributes: {},
    text: "",
    hidden: false,
    set textContent(value) {
      node.text = String(value);
    },
    get textContent() {
      return node.text;
    },
    get childElementCount() {
      return node.children.length;
    },
    get lastElementChild() {
      return node.children[node.children.length - 1] ?? null;
    },
    setAttribute(name, value) {
      node.attributes[name] = String(value);
    },
    append(...kids) {
      node.children.push(...kids);
    },
    appendChild(kid) {
      node.children.push(kid);
      return kid;
    },
    replaceChildren(...kids) {
      node.children = kids;
    },
  };
  return node;
}

globalThis.document ??= { createElement: element };

const { renderSlideMirror } = await import("../src/slides_mirror.mjs");
const { setCatalogue } = await import("../src/i18n.mjs");
const { EN_STRINGS } = await import("../src/en_strings.mjs");

setCatalogue("en", EN_STRINGS);

/** `tag[label]:text` for every element in a subtree, depth first. */
function outline(node, depth = 0) {
  const lines = [];
  for (const child of node.children) {
    const label = child.attributes["aria-label"];
    lines.push(
      `${"  ".repeat(depth)}${child.tagName.toLowerCase()}${label ? `[${label}]` : ""}` +
        (child.text ? `:${child.text}` : ""),
    );
    lines.push(...outline(child, depth + 1));
  }
  return lines;
}

function mirror(outlineJson) {
  const own = element("div");
  const inherited = element("div");
  const counts = renderSlideMirror({ own, inherited, outline: outlineJson });
  return { own, inherited, counts };
}

test("a slide's title becomes a HEADING, which is how a reader skims a deck", () => {
  const { own } = mirror({
    shapes: [
      {
        id: "1",
        tier: "slide",
        role: "title",
        name: "Title 1",
        paragraphs: [{ level: 0, text: "Quarterly review" }],
      },
    ],
  });
  // `h3`, under the page's own `<h2>` for the stage. A paragraph would leave the
  // deck with no heading structure at all, which is the difference between
  // skimming a forty-slide deck and reading all of it.
  assert.deepEqual(outline(own), ["h3:Quarterly review"]);
});

test("outline depth becomes NESTING, so an indented point is not announced as a top-level one", () => {
  const { own } = mirror({
    shapes: [
      {
        id: "2",
        tier: "slide",
        role: "body",
        name: "Content Placeholder 2",
        paragraphs: [
          { level: 0, text: "Revenue" },
          { level: 1, text: "By region" },
          { level: 1, text: "By product" },
          { level: 0, text: "Costs" },
        ],
      },
    ],
  });
  // One `ul` per depth, the deeper list INSIDE the item above it. Emitting the
  // four as siblings tells assistive technology an indented outline is flat,
  // which is a different claim about the slide than the one it makes.
  assert.deepEqual(outline(own), [
    "div[Content Placeholder 2]",
    "  ul",
    "    li:Revenue",
    "      ul",
    "        li:By region",
    "        li:By product",
    "    li:Costs",
  ]);
});

test("a shape with no depth is PARAGRAPHS, not a one-item list", () => {
  const { own } = mirror({
    shapes: [
      {
        id: "3",
        tier: "slide",
        role: "shape",
        name: "",
        paragraphs: [
          { level: 0, text: "First line" },
          { level: 0, text: "Second line" },
        ],
      },
    ],
  });
  // Wrapping flat text in a `ul` makes a reader hear "list, two items" before
  // every caption in the deck, which is noise standing in for information. And an
  // unnamed shape gets no group: a wrong name is worse than none.
  assert.deepEqual(outline(own), ["p:First line", "p:Second line"]);
});

test("a gap in depth is FILLED rather than dropping the paragraph", () => {
  const { own } = mirror({
    shapes: [
      {
        id: "4",
        tier: "slide",
        role: "body",
        name: "",
        paragraphs: [{ level: 2, text: "Orphaned sub-point" }],
      },
    ],
  });
  // A real deck states this whenever an author deletes the middle row. The words
  // survive — losing them is the one outcome worse than an odd structure.
  assert.deepEqual(outline(own), [
    "ul",
    "  li",
    "    ul",
    "      li",
    "        ul",
    "          li:Orphaned sub-point",
  ]);
});

test("the deck's repeated furniture is its OWN region, counted and labelled", () => {
  const { own, inherited, counts } = mirror({
    shapes: [
      {
        id: "5",
        tier: "slide",
        role: "title",
        name: "Title 1",
        paragraphs: [{ level: 0, text: "Opening" }],
      },
      {
        id: "6",
        tier: "layout",
        role: "shape",
        name: "Footer rule",
        paragraphs: [{ level: 0, text: "opendoc" }],
      },
      {
        id: "7",
        tier: "master",
        role: "shape",
        name: "",
        paragraphs: [{ level: 0, text: "Confidential" }],
      },
    ],
  });
  // Interleaved, a reader hears the same footer between every pair of slides with
  // no way to tell it from the slide's own words. So the slide's region holds
  // only the slide's shapes.
  assert.deepEqual(outline(own), ["h3:Opening"]);
  assert.deepEqual(outline(inherited), ["div[Footer rule]", "  p:opendoc", "p:Confidential"]);
  assert.equal(inherited.hidden, false);
  // The count reaches the region's accessible name, because a reader deciding
  // whether to walk into it wants to know how much is in there — and it is the
  // part of the name a markup-declared label cannot carry.
  assert.equal(inherited.attributes["aria-label"], "Repeated on every slide: 2 items");
  assert.deepEqual(counts, { own: 1, inherited: 2 });
});

test("a deck with no inherited text HIDES the region rather than announcing an empty one", () => {
  const { inherited } = mirror({
    shapes: [
      { id: "8", tier: "slide", role: "title", name: "", paragraphs: [{ level: 0, text: "Ten" }] },
    ],
  });
  assert.equal(inherited.hidden, true, "an empty labelled region announces itself as one");
});

test("a malformed or absent projection empties the mirror instead of throwing", () => {
  // `slides.mjs` yields `null` for a projection it cannot parse, and losing the
  // whole page over it would be the wrong trade — the slides still render.
  for (const bad of [null, undefined, {}, { shapes: "not an array" }]) {
    const { own, inherited, counts } = mirror(bad);
    assert.deepEqual(outline(own), []);
    assert.equal(inherited.hidden, true);
    assert.deepEqual(counts, { own: 0, inherited: 0 });
  }
  // A shape carrying no paragraphs contributes nothing: an empty heading or an
  // empty group is a stop on a reader's heading list that says nothing.
  const { own } = mirror({
    shapes: [{ id: "9", tier: "slide", role: "title", name: "Title 1", paragraphs: [] }],
  });
  assert.deepEqual(outline(own), []);
});
