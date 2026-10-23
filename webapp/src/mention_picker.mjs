// The @mention picker in a comment or reply composer (`docs/175` §7, `docs/109`
// HF-361) — Word's and Google Docs' people picker, over the HOST's directory.
//
// What it does, in the order a reader meets it:
//   * typing `@` and at least one more character after a space (or at the start)
//     asks the host who matches, after typing has settled for 150 ms; each
//     keystroke aborts the question before it, so a slow directory never paints
//     a stale list over a newer one;
//   * the answer is a listbox under the composer: ArrowUp/ArrowDown move, Enter
//     or Tab picks, Escape closes it (and only it — the composer stays open);
//   * picking replaces `@query` with `@Name ` and remembers the person, so the
//     posted comment links that span to their entry (`placeMentions`).
//
// With no host directory this attaches nothing at all: no listener, no element,
// and `@` is text. Avatars are not drawn here: a directory picture is a fetch the
// mount has not been asked to consent to in this surface, so the chip is initials.
//
// Complexity: O(limit) per answer, O(text) per keystroke for the query match.
import {
  DIRECTORY_DEBOUNCE_MS,
  DIRECTORY_LIMIT,
  mentionQuery,
  placeMentions,
} from "./directory.mjs";

let pickerCount = 0;

/**
 * @param {object} io
 * @param {HTMLTextAreaElement} io.textarea the composer.
 * @param {object|null|undefined} io.mentions the session's `mentions` seam.
 * @param {(key: string, params?: object) => string} io.t
 * @returns {{placed: () => Array<{start:number,end:number,link:string|null,id:string}>, detach: () => void}}
 */
export function attachMentionPicker({ textarea, mentions, t, setTimer = setTimeout, clearTimer = clearTimeout }) {
  const picked = [];
  const none = { placed: () => [], picked: () => [], detach() {} };
  if (!textarea || !mentions?.available?.()) return none;

  const id = `mention-list-${(pickerCount += 1)}`;
  const list = textarea.ownerDocument.createElement("div");
  list.id = id;
  list.className = "mention-list";
  list.setAttribute("role", "listbox");
  list.hidden = true;
  textarea.setAttribute("aria-autocomplete", "list");
  textarea.setAttribute("aria-controls", id);
  textarea.setAttribute("aria-expanded", "false");
  // Placed beside the composer when it first shows: a composer is usually built
  // before it is put in the page, and an element cannot sit after a parentless one.

  let timer = null;
  let controller = null;
  let query = null;
  let options = [];
  /** The entries the options stand for, index for index. */
  let shown = [];
  let active = -1;

  function close() {
    if (timer !== null) clearTimer(timer);
    timer = null;
    controller?.abort();
    controller = null;
    query = null;
    options = [];
    shown = [];
    active = -1;
    list.hidden = true;
    list.replaceChildren();
    textarea.setAttribute("aria-expanded", "false");
    textarea.removeAttribute("aria-activedescendant");
  }

  function highlight(index) {
    active = index;
    for (const [n, el] of options.entries()) el.setAttribute("aria-selected", String(n === index));
    if (index >= 0) textarea.setAttribute("aria-activedescendant", options[index].id);
    else textarea.removeAttribute("aria-activedescendant");
  }

  function render(asked, answer) {
    list.replaceChildren();
    options = [];
    shown = answer.people.slice();
    list.setAttribute("aria-label", t("mention.listLabel", { query: asked.query }));
    if (!answer.people.length) {
      const empty = textarea.ownerDocument.createElement("div");
      empty.className = "mention-empty";
      empty.textContent = t("mention.noMatch", { query: asked.query });
      list.append(empty);
    }
    for (const [n, person] of answer.people.entries()) {
      const option = textarea.ownerDocument.createElement("div");
      option.id = `${id}-${n}`;
      option.className = "mention-option";
      option.setAttribute("role", "option");
      option.setAttribute("aria-selected", "false");
      option.dataset.personId = person.id;
      const chip = textarea.ownerDocument.createElement("span");
      chip.className = "mention-initials";
      chip.setAttribute("aria-hidden", "true");
      chip.textContent = person.initials ?? initialsOf(person.name);
      const name = textarea.ownerDocument.createElement("span");
      name.textContent = person.name;
      option.append(chip, name);
      // `mousedown`, not `click`: picking must not blur the composer first.
      option.addEventListener("mousedown", (event) => { event.preventDefault(); pick(asked, person); });
      list.append(option);
      options.push(option);
    }
    if (list.parentNode !== textarea.parentNode) textarea.insertAdjacentElement("afterend", list);
    list.hidden = false;
    textarea.setAttribute("aria-expanded", "true");
    highlight(options.length ? 0 : -1);
  }

  async function ask(asked) {
    controller?.abort();
    controller = new AbortController();
    const mine = controller;
    let answer;
    try {
      answer = await mentions.search(asked.query, { limit: DIRECTORY_LIMIT, signal: mine.signal });
    } catch {
      if (mine.signal.aborted) return;
      answer = { people: [], more: false, dropped: 0 };
    }
    if (mine.signal.aborted || query?.query !== asked.query) return;
    if (answer.dropped) mentions.dropped?.(answer.dropped);
    render(asked, answer);
  }

  function pick(asked, person) {
    const spelled = `@${person.name} `;
    const caret = textarea.selectionStart ?? textarea.value.length;
    textarea.setRangeText(spelled, asked.start, caret, "end");
    picked.push({ id: person.id, name: person.name, link: person.link ?? null });
    close();
    textarea.dispatchEvent(new Event("input", { bubbles: true }));
    textarea.focus();
  }

  function onInput() {
    const next = mentionQuery(textarea.value, textarea.selectionStart);
    if (!next) { close(); return; }
    if (query && next.query === query.query && next.start === query.start) return;
    query = next;
    if (timer !== null) clearTimer(timer);
    timer = setTimer(() => { timer = null; void ask(next); }, DIRECTORY_DEBOUNCE_MS);
  }

  function onKey(event) {
    if (list.hidden) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (!options.length) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const step = event.key === "ArrowDown" ? 1 : -1;
      highlight((active + step + options.length) % options.length);
    } else if ((event.key === "Enter" || event.key === "Tab") && active >= 0 && !event.shiftKey) {
      event.preventDefault();
      event.stopImmediatePropagation();
      // The answer is already on screen; a keyboard pick asks no second question.
      pick(query, shown[active]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopImmediatePropagation();
      close();
    }
  }

  textarea.addEventListener("input", onInput);
  textarea.addEventListener("keydown", onKey);
  textarea.addEventListener("blur", close);

  return {
    /** The picked mentions where they now sit in the composer's text. */
    placed: () => placeMentions(textarea.value, picked),
    picked: () => picked.slice(),
    detach() {
      close();
      textarea.removeEventListener("input", onInput);
      textarea.removeEventListener("keydown", onKey);
      textarea.removeEventListener("blur", close);
      list.remove();
    },
  };
}

function initialsOf(name) {
  const words = String(name).trim().split(/\s+/u).filter(Boolean);
  const first = [...(words[0] ?? "")][0] ?? "";
  const last = words.length > 1 ? [...words[words.length - 1]][0] ?? "" : "";
  return (first + last).toLocaleUpperCase();
}
