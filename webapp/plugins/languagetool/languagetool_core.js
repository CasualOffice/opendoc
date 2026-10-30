// SPDX-License-Identifier: Apache-2.0
// LanguageTool for OpenDoc: the logic, with no DOM and no network (`docs/183` §18).
//
// LanguageTool (<https://languagetool.org>, LGPL-2.1) is a SERVER this add-in
// talks to over its documented HTTP API (`POST /v2/check`,
// <https://languagetool.org/http-api/>); none of its code is in this repository.
// By default that server is LanguageTool's public API; an administrator points
// the add-in at their own (`config.server` and `network` in the catalogue).
//
// Document text is sent in chunks of whole paragraphs; every match is mapped
// back to its paragraph and turned into a suggestion anchored on the exact text
// the server flagged. A match that spans two paragraphs, or whose text is no
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
        issueType: String((m.rule && m.rule.issueType) || ""),
      });
    });
    return { issues: issues, dropped: dropped };
  }

  /** One suggestion: replace the flagged text with `replacement`. */
  function editFor(issue, replacement) {
    return { op: "replace_text", target: { node: issue.node, exact: issue.exact, occurrence: issue.occurrence }, text: String(replacement) };
  }

  /**
   * Every issue with a replacement, as suggestions, at most `max`, never two on
   * overlapping text of one paragraph (the second would anchor on text the
   * first changed).
   */
  function planForAll(issues, revision, max) {
    var edits = [];
    var taken = {};
    (issues || []).forEach(function (issue) {
      if (edits.length >= (max || 100) || !issue.replacements.length) return;
      var spans = taken[issue.node] || (taken[issue.node] = []);
      var overlaps = spans.some(function (s) { return issue.offset < s[1] && s[0] < issue.offset + issue.length; });
      if (overlaps) return;
      spans.push([issue.offset, issue.offset + issue.length]);
      edits.push(editFor(issue, issue.replacements[0]));
    });
    return edits.length ? { ok: true, plan: { base_revision: revision, edits: edits } } : { ok: false, reason: "nothing" };
  }

  root.LanguageToolCore = Object.freeze({
    DEFAULT_SERVER: DEFAULT_SERVER,
    chunks: chunks,
    requestBody: requestBody,
    checkUrl: checkUrl,
    issuesFrom: issuesFrom,
    editFor: editFor,
    planForAll: planForAll,
  });
})(typeof self !== "undefined" ? self : globalThis);
