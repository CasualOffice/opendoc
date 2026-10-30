// SPDX-License-Identifier: Apache-2.0
// LanguageTool for OpenDoc: the logic, with no DOM and no network (`docs/183` §18, §19).
//
// LanguageTool (<https://languagetool.org>, LGPL-2.1) is a SERVER this add-in
// talks to over its documented HTTP API (`POST /v2/check`,
// <https://languagetool.org/http-api/>); none of its code is in this repository.
// By default that server is LanguageTool's public API; an administrator points
// the add-in at their own (`config.server` and `network` in the catalogue).
//
// The editor sends the paragraphs that changed; they go to the server in chunks
// of whole paragraphs, and every match is mapped back to its paragraph as a
// writing issue the EDITOR draws in the text and offers in its own card. A match that spans two paragraphs, or whose text is no
// longer what the paragraph holds, is dropped and counted rather than guessed.
(function (root) {
  "use strict";

  var SEPARATOR = "\n\n";
  var DEFAULT_SERVER = "https://api.languagetool.org";

  /** Paragraph blocks grouped into chunks of at most `maxChars` characters. */
  function chunks(blocks, maxChars) {
    var limit = maxChars || 6000;
    var out = [];
    var current = { text: "", map: [] };
    (blocks || []).forEach(function (b) {
      if (!b || b.kind !== "paragraph" || typeof b.text !== "string" || !b.text.trim()) return;
      var text = b.text.length > limit ? b.text.slice(0, limit) : b.text;
      if (current.text && current.text.length + SEPARATOR.length + text.length > limit) {
        out.push(current);
        current = { text: "", map: [] };
      }
      var start = current.text ? current.text.length + SEPARATOR.length : 0;
      current.text = current.text ? current.text + SEPARATOR + text : text;
      current.map.push({ node: b.id, start: start, length: text.length, text: b.text });
    });
    if (current.text) out.push(current);
    return out;
  }

  /** The form body of one `/v2/check` request. */
  function requestBody(text, language) {
    var params = new URLSearchParams();
    params.set("text", text);
    params.set("language", language || "auto");
    return params.toString();
  }

  /** The check URL for a server (`https://host[/path]`). */
  function checkUrl(server) {
    return String(server || DEFAULT_SERVER).replace(/\/+$/, "") + "/v2/check";
  }

  /** The 1-based occurrence of `exact` in `text` that starts at `offset`. */
  function occurrenceAt(text, exact, offset) {
    var n = 0;
    for (var at = text.indexOf(exact); at >= 0 && at <= offset; at = text.indexOf(exact, at + 1)) {
      n += 1;
      if (at === offset) return n;
    }
    return 0;
  }

  /**
   * The server's matches for one chunk, mapped to paragraphs:
   * `{issues: [{node, offset, length, exact, occurrence, message, short,
   * replacements, rule, category, issueType}], dropped}`.
   */
  function issuesFrom(response, chunk) {
    var issues = [];
    var dropped = 0;
    var matches = (response && Array.isArray(response.matches) && response.matches) || [];
    matches.forEach(function (m) {
      var offset = Number(m.offset);
      var length = Number(m.length);
      var para = null;
      for (var i = 0; i < chunk.map.length; i += 1) {
        var p = chunk.map[i];
        if (offset >= p.start && offset + length <= p.start + p.length) { para = p; break; }
      }
      if (!para || !(length > 0)) { dropped += 1; return; }
      var local = offset - para.start;
      var exact = para.text.slice(local, local + length);
      var occurrence = occurrenceAt(para.text, exact, local);
      if (!exact || !occurrence) { dropped += 1; return; }
      issues.push({
        node: para.node,
        offset: local,
        length: length,
        exact: exact,
        occurrence: occurrence,
        message: String(m.message || ""),
        short: String(m.shortMessage || ""),
        replacements: (m.replacements || []).slice(0, 5).map(function (r) { return String(r && r.value != null ? r.value : ""); }).filter(function (v) { return v !== exact; }),
        rule: String((m.rule && m.rule.id) || ""),
        category: String((m.rule && m.rule.category && m.rule.category.name) || ""),
        categoryId: String((m.rule && m.rule.category && m.rule.category.id) || ""),
        issueType: String((m.rule && m.rule.issueType) || ""),
      });
    });
    return { issues: issues, dropped: dropped };
  }

  /**
   * The editor's category for a LanguageTool match (`docs/183` §19): spelling,
   * punctuation, style or grammar, from the rule's documented `issueType` and
   * category id (<https://languagetool.org/http-api/>).
   */
  var STYLE_TYPES = ["style", "register", "duplication", "locale-violation", "inconsistency"];
  var STYLE_CATEGORIES = ["STYLE", "REDUNDANCY", "PLAIN_ENGLISH", "COLLOQUIALISMS", "REPETITIONS_STYLE", "WIKIPEDIA"];
  function categoryOf(issueType, categoryId) {
    var type = String(issueType || "").toLowerCase();
    var id = String(categoryId || "").toUpperCase();
    if (type === "misspelling" || id === "TYPOS") return "spelling";
    if (type === "typographical" || type === "whitespace" || id === "PUNCTUATION" || id === "TYPOGRAPHY") return "punctuation";
    if (STYLE_TYPES.indexOf(type) >= 0 || STYLE_CATEGORIES.indexOf(id) >= 0) return "style";
    return "grammar";
  }

  /**
   * What the add-in reports for the paragraphs it was sent: one result per
   * paragraph (an empty list clears its marks), each match in the editor's
   * shape with UTF-16 offsets into the paragraph's text.
   */
  function resultsFor(paragraphs, issues) {
    var byNode = {};
    (paragraphs || []).forEach(function (p) { byNode[p.node] = { node: p.node, text: p.text, matches: [] }; });
    (issues || []).forEach(function (issue) {
      var result = byNode[issue.node];
      if (!result) return;
      result.matches.push({
        start: issue.offset,
        end: issue.offset + issue.length,
        exact: issue.exact,
        category: categoryOf(issue.issueType, issue.categoryId),
        message: issue.message || issue.short,
        replacements: issue.replacements.slice(0, 3),
        rule: issue.rule,
      });
    });
    return Object.keys(byNode).map(function (k) { return byNode[k]; });
  }

  /** The server's language code for a paragraph's `w:lang`, or "auto". */
  function languageFor(tag) {
    var t = String(tag || "");
    return /^[a-z]{2,3}(-[A-Za-z]{2,4})?$/.test(t) ? t : "auto";
  }

  root.LanguageToolCore = Object.freeze({
    DEFAULT_SERVER: DEFAULT_SERVER,
    chunks: chunks,
    requestBody: requestBody,
    checkUrl: checkUrl,
    issuesFrom: issuesFrom,
    categoryOf: categoryOf,
    resultsFor: resultsFor,
    languageFor: languageFor,
  });
})(typeof self !== "undefined" ? self : globalThis);
