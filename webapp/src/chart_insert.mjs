/** Inserting a chart, and the two facts the host must decide before it can.
 *
 *  The engine has had `insertChart` since #761 and no surface reached it, so
 *  "charts are insertable" was an ENGINE capability published as a product one
 *  (`SKILL.md` §9 rule 4). This module is the part that is not `main.js` state:
 *  which family a new chart is, and how the engine's refusal becomes a sentence.
 *
 *  Competitive standard first, per the editing rule. Word's Insert ▸ Chart opens
 *  a type gallery and then a data sheet; Google Docs' Insert ▸ Chart offers Bar,
 *  Column, Line, Pie and "From Sheets". Neither is reachable here yet, because
 *  this build had no chart DATA editor — a type picker that hands the reader
 *  seven variations of uneditable placeholder data would be choice without
 *  consequence. So the first surface inserted Word's own default, a clustered
 *  column chart.
 *
 *  The data editor shipped (#811), so the type picker is no longer choice
 *  without consequence, and Insert ▸ Chart now opens it first (`109` HF-258):
 *  ONLYOFFICE's Insert Chart button carries a chart-type picker as its menu
 *  (`web-apps/apps/documenteditor/main/app/view/Toolbar.js:2782`, the
 *  `menu-insertchart` DataView over `Common.define.chartData`), and Google
 *  Docs' Insert ▸ Chart is a submenu of families. The picker here is the SAME
 *  grouped list the Chart tab's Change-type menu shows (`CHART_TYPE_GROUPS`),
 *  so a type that can be chosen after insert can be chosen at insert.
 */
import { CHART_TYPE_GROUPS, CHART_TYPE_KEYS } from "./chart_commands.mjs";

/** The family a new chart is, until a picker exists.
 *
 *  Word's default for Insert ▸ Chart is a clustered column chart, and `column`
 *  is one of the seven `chart_group_for_kind` accepts.
 */
export const DEFAULT_CHART_KIND = "column";

/** Turns the engine's chart refusal into something a reader can act on.
 *
 *  The engine marks its refusals (`chart.unpainted-family`), and those are
 *  already readable sentences naming the families that do work — so this does
 *  not rewrite them. It only covers the case the engine cannot phrase, where the
 *  failure arrived with no message at all.
 *
 *  @param {unknown} err
 */
export function chartInsertFailure(err) {
  const text = typeof err === "string" ? err : (err?.message ?? "");
  return text.trim() === "" ? "Could not insert the chart." : text;
}

/** Inserts a chart at the caret, or says why it did not.
 *
 *  Every dependency is injected because all of them are `main.js` state — the
 *  document handle, the caret, the two refusal predicates, the status line and
 *  the apply path. That is what lets the whole insert PATH be tested, including
 *  the two refusals, rather than only the engine call underneath it.
 *
 *  O(1) in document size: one engine call plus whatever `apply` repaints.
 *
 *  @param {object} o
 *  @param {{insertChart: Function} | null} o.doc
 *  @param {{node: string, offset: number} | null | undefined} o.caret
 *  @param {() => boolean} o.blocked      viewing mode refuses every mutation
 *  @param {() => boolean} o.suggesting   tracked-change mode cannot record this yet
 *  @param {(text: string, kind: string) => void} o.status
 *  @param {(result: unknown) => Promise<unknown>} o.apply
 *  @returns {Promise<boolean>} whether a chart was inserted — the caller then
 *           selects it and opens its data, which is Word's Insert ▸ Chart.
 */
export async function insertChartAtCaret({ doc, caret, blocked, suggesting, status, apply, kind = DEFAULT_CHART_KIND }) {
  if (!doc || !caret || blocked()) return false;
  if (suggesting()) {
    // Not a dead control and not a silent no-op: the reader is told which mode
    // refuses and which one to switch to, as `insertNote` does for notes.
    status("Inserting a chart cannot be tracked yet; switch to Editing", "error");
    return false;
  }
  try {
    await apply(doc.insertChart(caret.node, caret.offset, kind));
  } catch (err) {
    status(chartInsertFailure(err), "error");
    return false;
  }
  return true;
}

/** The Insert ▸ Chart type picker as a command list, in ONLYOFFICE's picker
 *  order and with one menu group per family, so the menu separates them the way
 *  their DataView groups do.
 *
 *  Every entry is a family token the engine's `chart_group_for_kind` admits —
 *  the same list the Chart tab's Change type offers — and each runs `insert`
 *  with its token. The menu is the shared context menu, so it is keyboard
 *  operable (arrows, Enter, Escape) and every row is a `menuitem` with its
 *  translated name.
 *
 *  O(types): seventeen rows.
 *
 *  @param {(key: string, params?: object) => string} t
 *  @param {(kind: string) => unknown} insert
 *  @returns {{id: string, label: string, group: string, enabled: boolean, run: () => unknown}[]}
 */
export function insertChartTypeCommands(t, insert) {
  return CHART_TYPE_GROUPS.flatMap((group) =>
    group.kinds.map((kind) => ({
      id: `insert.chart.${kind}`,
      label: t(CHART_TYPE_KEYS[kind] ?? "chart.kind.column"),
      group: group.key,
      enabled: true,
      run: () => insert(kind),
    })),
  );
}
