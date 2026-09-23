// The footer's counts, as sentences (docs/124).
//
// Pure: it takes four numbers and returns the strings the status bar shows.
// The reason it is a module rather than four lines in `updateStats` is that
// the same four figures are rendered FIVE times — three visible counts, the
// characters tooltip, and the whole-region tooltip that carries everything the
// narrow-window ladder has shed — and they were five separately written
// sentences with the plural rule hand-rolled into each. One of them is now one
// call, and a translator sees one set of keys.
import { t } from "./i18n.mjs";

/**
 * @param {{words: number, characters: number, charactersNoSpaces: number, paragraphs: number}} stats
 * @returns {{words: string, characters: string, paragraphs: string, charactersTitle: string, allTitle: string}}
 */
export function countLabels({ words, characters, charactersNoSpaces, paragraphs }) {
  const withSpaces = t("status.characters.withSpaces", { count: characters });
  const noSpaces = t("status.characters.noSpaces", { count: charactersNoSpaces });
  return {
    words: t("status.words", { count: words }),
    characters: t("status.characters", { count: characters }),
    paragraphs: t("status.paragraphs", { count: paragraphs }),
    // Word distinguishes with- from without-spaces; both are on the hover.
    charactersTitle: `${withSpaces}\n${noSpaces}`,
    // Narrow windows shed the lower-priority counts from the bar. Word keeps
    // the full set one gesture away in its Word Count dialog; until we have
    // that dialog, the whole region carries every figure so nothing shed
    // becomes unobtainable.
    allTitle: [
      t("status.words", { count: words }),
      withSpaces,
      noSpaces,
      t("status.paragraphs", { count: paragraphs }),
    ].join("\n"),
  };
}
