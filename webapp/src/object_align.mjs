// Arrange ▸ Align and Distribute for floating objects (`docs/109` RM-28).
//
// Competitive standard first: ONLYOFFICE's Align dropdown
// (`web-apps/apps/documenteditor/main/app/view/Toolbar.js`, `mniAlignToPage`,
// `mniAlignToMargin`, `mniAlignObjects`, `mniDistribHor`, `mniDistribVert`) and
// Word's Arrange ▸ Align both list six alignments, two distributions and a
// three-way target — the page, the margin or the selected objects — that stays
// chosen. Word's default target is the margin with one object selected and the
// selected objects with several; the same rule is `defaultAlignTarget`.
//
// The engine (`alignObjects` / `canAlignObjects`, `casual-doc-wasm`'s
// `object_align.rs`) does the geometry and writes every object's anchor in ONE
// undoable action. This module is the menu: which rows, which target, which
// reason a row is closed for, and what the status line says afterwards.
//
// `io`:
//   `doc()`            the engine handle
//   `nodes()`          the selected roots, primary first (`objectArrange.candidates`)
//   `runEdit(fn, o)`   the gated edit path (resolves to a bool)
//   `t(key, params)`
//   `explain(raw)`     an engine refusal as the reader's sentence
//   `announce(text)`   the status line, which a screen reader hears
//   `modifier`         the localised Ctrl/⌘ label
//
// Cost: TWO `canAlignObjects` reads per menu build (one for the alignments, one
// for the distributions), each O(selected × document); nothing per row and
// nothing on a pointer path.

/** The six alignments and two distributions, in Word's and ONLYOFFICE's order. */
export const ALIGN_CHOICES = Object.freeze([
  Object.freeze({ how: "left", key: "object.align.left", group: "align" }),
  Object.freeze({ how: "center", key: "object.align.center", group: "align" }),
  Object.freeze({ how: "right", key: "object.align.right", group: "align" }),
  Object.freeze({ how: "top", key: "object.align.top", group: "align" }),
  Object.freeze({ how: "middle", key: "object.align.middle", group: "align" }),
  Object.freeze({ how: "bottom", key: "object.align.bottom", group: "align" }),
  Object.freeze({ how: "distributeHorizontally", key: "object.align.distributeHorizontally", group: "distribute" }),
  Object.freeze({ how: "distributeVertically", key: "object.align.distributeVertically", group: "distribute" }),
]);

/** The three targets, as their catalogue keys. */
export const ALIGN_TARGETS = Object.freeze([
  Object.freeze({ target: "page", key: "object.align.toPage" }),
  Object.freeze({ target: "margin", key: "object.align.toMargin" }),
  Object.freeze({ target: "selected", key: "object.align.toSelected" }),
]);

/** Word's default: the margin for one object, the selection for several. */
export function defaultAlignTarget(count) {
  return count > 1 ? "selected" : "margin";
}

/** The target in force: the reader's choice while it can apply, else the
 *  default. "Selected objects" cannot apply to one object, so a reader who
 *  chose it and then selects a single picture gets the margin, as in Word. */
export function effectiveAlignTarget(chosen, count) {
  if (!chosen || (chosen === "selected" && count < 2)) return defaultAlignTarget(count);
  return chosen;
}

function verdict(json) {
  try {
    const parsed = JSON.parse(json ?? "");
    return { can: parsed.can === true, reason: parsed.reason ?? "" };
  } catch {
    return { can: false, reason: "" };
  }
}

export function createObjectAlign(io) {
  /** The target the reader picked, or null for Word's default. */
  let chosen = null;

  /**
   * The Align submenu's rows for the current selection.
   *
   * @param {boolean} mutationEnabled  the review-mode gate
   * @param {string} mutationReason    why it is closed, when it is
   */
  function rows(mutationEnabled, mutationReason) {
    const nodes = io.nodes();
    const json = JSON.stringify(nodes);
    const target = effectiveAlignTarget(chosen, nodes.length);
    const doc = io.doc();
    const closed = (raw, fallbackKey) => ({
      enabled: false,
      disabledReason: raw ? io.explain(raw) : io.t(fallbackKey, { modifier: io.modifier }),
    });
    const gateFor = (how, minimum, fallbackKey) => {
      if (!mutationEnabled) return { enabled: false, disabledReason: mutationReason };
      // The count rule is answered here so the sentence says what to DO.
      if (nodes.length < minimum) return closed("", fallbackKey);
      const answer = verdict(doc?.canAlignObjects?.(json, how, target));
      return answer.can ? { enabled: true } : closed(answer.reason, fallbackKey);
    };
    const alignGate = gateFor("left", target === "selected" ? 2 : 1, "object.align.reason.selectMore");
    const distributeGate = gateFor("distributeHorizontally", 3, "object.align.reason.selectThree");
    const out = ALIGN_CHOICES.map((choice) => ({
      id: `object.align.${choice.how}`,
      label: io.t(choice.key),
      group: choice.group,
      ...(choice.group === "distribute" ? distributeGate : alignGate),
      run: () => run(choice.how),
    }));
    for (const option of ALIGN_TARGETS) {
      const available = option.target !== "selected" || nodes.length > 1;
      out.push({
        id: `object.align.to.${option.target}`,
        label: io.t(option.key),
        group: "target",
        shortcut: target === option.target ? "✓" : "",
        enabled: available,
        disabledReason: available ? "" : io.t("object.align.reason.selectMore", { modifier: io.modifier }),
        run: () => {
          chosen = option.target;
          io.announce(io.t(option.key));
        },
      });
    }
    return out;
  }

  /** Aligns or distributes the selection against the target in force. */
  async function run(how) {
    const nodes = io.nodes();
    const target = effectiveAlignTarget(chosen, nodes.length);
    const applied = await io.runEdit(() => io.doc().alignObjects(JSON.stringify(nodes), how, target), { gate: true });
    if (applied) {
      io.announce(io.t(how.startsWith("distribute") ? "object.align.distributed" : "object.align.aligned"));
    }
    return applied;
  }

  return { rows, run, target: () => effectiveAlignTarget(chosen, io.nodes().length) };
}
