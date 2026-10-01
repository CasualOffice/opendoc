//! What an [`InlineNode`] CONTAINS — the one place in this crate that knows the
//! inline container set (`docs/109` HF-212).
//!
//! # The class this closes
//!
//! An inline can contain other inlines, or block content of its own. The complete
//! container set is **six**: `Hyperlink`, `Field`, `Revision`, `Sdt` (inline in
//! inline) and `TextBox`, `Group` (block in inline). Written out ad hoc, each walk
//! implemented three or four of the six and stopped behind a `_ =>` arm — and the
//! matrix over this crate said so: of the walks over `InlineNode` in
//! `lib.rs`, **exactly one entered all six, and 23 carried a catch-all**. Every
//! walk decided the set for itself, silently, which is why one defect kept
//! arriving in a new place: HF-191 (25 of 29 inline kinds dropped by the only deep
//! copy), HF-194/195 (`Sdt`/`Revision`, so a text box in a content control is not
//! an editing surface), HF-196 (`Group`, exporting a dangling relationship),
//! HF-209 (an entity reference in a footer). Four in one day, all the same shape.
//!
//! The fix is not another arm. It is one declared set that a walk consults, plus
//! `every_inline_walk_consults_the_container_set_or_says_why_not` in this file's
//! own test module, which fails the build when a walk over `InlineNode` anywhere in
//! this crate neither consults this module nor states in one line why it does not.
//!
//! # This is `casual-doc-wasm`'s shape, deliberately — not a second one
//!
//! `casual-doc-wasm` closed the same class in its own crate first (#651) with
//! `InlineDescent`/`inline_descent`, `contained_inlines`, `contained_inlines_mut`,
//! `inline_block_stories` and `group_block_stories`. This module carries the same
//! names, the same four axes and the same no-wildcard rule, because two solutions
//! to one problem is the thing this repository keeps paying for. It adds only what
//! an *editing* crate additionally needs and a read-only façade does not: the
//! **mutable** descent ([`InlineDescentMut`]), which hands out `&mut` children on
//! all three axes rather than only the inline one.
//!
//! The two crates cannot share one copy today: the container set is a fact about
//! `casual_doc_model::v1::InlineNode` and belongs in `casual-doc-model`, which
//! neither crate owns. Reported as a row rather than taken, so until it moves there
//! are two copies of one rule — and each crate's own guard holds its copy to the
//! same shape, which is the most either can do from where it sits.
//!
//! # The axes, and why a walk must pick one
//!
//! Not every walk should descend everything, and the difference is not taste:
//!
//! * **inline axis** ([`InlineDescent::Inlines`]) — children in the SAME paragraph
//!   and the same model-offset space. Anything that measures or addresses byte
//!   offsets within one paragraph follows only this one.
//! * **block-story axis** ([`InlineDescent::Blocks`], [`InlineDescent::Group`]) — a
//!   text box's or a group's own paragraphs. These are *separate stories*: their
//!   paragraphs have ids in the same document-wide space but offsets in a
//!   different space, so a length walk that followed them would report a
//!   paragraph longer than any offset the host can produce, and a caption inside a
//!   text box would be counted twice.
//!
//! So "the choke point" does not mean "descend everything". It means one place
//! that says, per variant, *what descending would be*, with each caller picking
//! the axis its question needs.
//!
//! # Complexity
//!
//! Every function here is **O(1)** except [`find_in_group_block_stories`] and
//! [`find_in_group_block_stories_mut`], which are O(children in the group
//! subtree). Nothing here resolves a `NodeId`, so nothing here is a document
//! scan: these are the primitives that let a per-interaction walk stay O(1) in
//! document size (`docs/107` §4).

use casual_doc_model::v1::{BlockNode, GroupChild, InlineNode};

/// What an [`InlineNode`] contains, by descent axis.
///
/// The match in [`inline_descent`] is **exhaustive with no wildcard arm**: a
/// wildcard is how the next inline kind gets silently treated as a leaf, which is
/// this same defect arriving by omission.
pub(crate) enum InlineDescent<'a> {
    /// Inline children in the SAME paragraph and the same model-offset space:
    /// `Hyperlink`, `Field`, `Revision`, `Sdt`.
    Inlines(&'a [InlineNode]),
    /// A block story of its own — `TextBox`.
    Blocks(&'a [BlockNode]),
    /// A DrawingML group's children — `Group`. Pictures and shapes are leaves; its
    /// text boxes and nested groups carry block stories.
    Group(&'a [GroupChild]),
    /// Everything else: a leaf, with nothing inside it to visit.
    Leaf,
}

/// The children of one inline node, by descent axis. **O(1)** — it returns
/// borrowed slices and visits nothing.
///
/// `Revision` is a container whatever its kind: a walk looking for *content*
/// decides for itself whether a non-contributing revision counts (see
/// `editing_transparent` in `lib.rs`), but a walk looking for an *id* must descend
/// one regardless, because a tracked deletion's nodes are still in the tree.
pub(crate) fn inline_descent(inline: &InlineNode) -> InlineDescent<'_> {
    match inline {
        InlineNode::Hyperlink(link) => InlineDescent::Inlines(&link.inlines),
        InlineNode::Field(field) => InlineDescent::Inlines(&field.inlines),
        InlineNode::Revision(revision) => InlineDescent::Inlines(&revision.inlines),
        InlineNode::Sdt(sdt) => InlineDescent::Inlines(&sdt.inlines),
        InlineNode::TextBox(text_box) => InlineDescent::Blocks(&text_box.blocks),
        InlineNode::Group(group) => InlineDescent::Group(&group.children),
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::PositionalTab(_)
        | InlineNode::Break(_)
        | InlineNode::Symbol(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::Math(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => InlineDescent::Leaf,
    }
}

/// [`InlineDescent`] for a walk that MUTATES what it finds.
///
/// Rust cannot share one match across mutability, so this is the one deliberate
/// second spelling of the container set, kept immediately beside the first so the
/// two cannot drift apart unnoticed — and
/// `the_two_faces_of_the_container_set_agree` fails the build if they ever do.
/// Adding a third spelling anywhere else is the defect this module is about.
pub(crate) enum InlineDescentMut<'a> {
    /// See [`InlineDescent::Inlines`].
    Inlines(&'a mut Vec<InlineNode>),
    /// See [`InlineDescent::Blocks`].
    Blocks(&'a mut Vec<BlockNode>),
    /// See [`InlineDescent::Group`].
    Group(&'a mut Vec<GroupChild>),
    /// See [`InlineDescent::Leaf`].
    Leaf,
}

/// [`inline_descent`] for a walk that MUTATES what it finds. **O(1)**.
pub(crate) fn inline_descent_mut(inline: &mut InlineNode) -> InlineDescentMut<'_> {
    match inline {
        InlineNode::Hyperlink(link) => InlineDescentMut::Inlines(&mut link.inlines),
        InlineNode::Field(field) => InlineDescentMut::Inlines(&mut field.inlines),
        InlineNode::Revision(revision) => InlineDescentMut::Inlines(&mut revision.inlines),
        InlineNode::Sdt(sdt) => InlineDescentMut::Inlines(&mut sdt.inlines),
        InlineNode::TextBox(text_box) => InlineDescentMut::Blocks(&mut text_box.blocks),
        InlineNode::Group(group) => InlineDescentMut::Group(&mut group.children),
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::PositionalTab(_)
        | InlineNode::Break(_)
        | InlineNode::Symbol(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::Math(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => InlineDescentMut::Leaf,
    }
}

/// The inline children an inline contains IN THE SAME PARAGRAPH, or `None` when it
/// has none. **O(1)**.
///
/// [`InlineDescent::Inlines`] on its own, for the walks that must follow only that
/// axis: anything measuring or addressing offsets within one paragraph. A
/// `TextBox`'s or a `Group`'s paragraphs are separate stories with their own
/// offset spaces, so following them would produce an offset in one paragraph for a
/// node that lives in another.
pub(crate) fn contained_inlines(inline: &InlineNode) -> Option<&[InlineNode]> {
    match inline_descent(inline) {
        InlineDescent::Inlines(inlines) => Some(inlines),
        InlineDescent::Blocks(_) | InlineDescent::Group(_) | InlineDescent::Leaf => None,
    }
}

/// [`contained_inlines`] for a walk that MUTATES what it finds. **O(1)**.
pub(crate) fn contained_inlines_mut(inline: &mut InlineNode) -> Option<&mut Vec<InlineNode>> {
    match inline_descent_mut(inline) {
        InlineDescentMut::Inlines(inlines) => Some(inlines),
        InlineDescentMut::Blocks(_) | InlineDescentMut::Group(_) | InlineDescentMut::Leaf => None,
    }
}

/// The first `Some` that `f` returns over the block stories a group's children
/// own, in paint order — the [`InlineDescent::Group`] axis flattened to the
/// stories inside it. **O(children in the subtree)**; a picture or a shape is a
/// leaf and carries no story.
///
/// For the walks whose question is about *block content*: a picture inside a text
/// box inside a group is inside a paragraph, and nothing but this reaches it.
/// A walk whose question is about the group children THEMSELVES (a shape's
/// geometry, a nested group's transform) takes [`InlineDescent::Group`] raw
/// instead and writes its own four-arm match over `GroupChild`, which the compiler
/// already makes exhaustive.
pub(crate) fn find_in_group_block_stories<'a, T>(
    children: &'a [GroupChild],
    f: &mut impl FnMut(&'a [BlockNode]) -> Option<T>,
) -> Option<T> {
    for child in children {
        let found = match child {
            GroupChild::TextBox(text_box) => f(&text_box.blocks),
            GroupChild::Group(nested) => find_in_group_block_stories(&nested.children, f),
            GroupChild::Picture(_) | GroupChild::Shape(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// [`find_in_group_block_stories`] for a walk that MUTATES what it finds.
/// **O(children in the subtree)**.
pub(crate) fn find_in_group_block_stories_mut<T>(
    children: &mut [GroupChild],
    f: &mut impl FnMut(&mut Vec<BlockNode>) -> Option<T>,
) -> Option<T> {
    for child in children {
        let found = match child {
            GroupChild::TextBox(text_box) => f(&mut text_box.blocks),
            GroupChild::Group(nested) => find_in_group_block_stories_mut(&mut nested.children, f),
            GroupChild::Picture(_) | GroupChild::Shape(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    // No `use super::*`: every guard here reads this crate's SOURCE rather than
    // calling into it, which is the point — the rule is about how walks are
    // written, and the behavioural proofs live beside the operations they cover in
    // `lib.rs`.

    /// The names that count as consulting the declared container set. A walk that
    /// mentions one of these gets its descent from this module and therefore
    /// cannot be missing a container.
    ///
    /// `transparent_children` and `editing_transparent` are here because they are
    /// *axes of* this module rather than second copies of it: the children come
    /// from [`contained_inlines`] and only the policy is theirs.
    const CHOKE_POINT: [&str; 5] = [
        "inline_descent",
        "contained_inlines",
        "find_in_group_block_stories",
        "transparent_children",
        "editing_transparent",
    ];

    /// The one-line escape hatch, for a walk that deliberately does not need the
    /// container set. Its text must name at least one container, so the author has
    /// to engage with the set rather than write "n/a".
    const DECLARATION: &str = "container-set:";

    /// The complete container set. A declaration has to name one of these.
    const CONTAINERS: [&str; 6] = ["Hyperlink", "Field", "TextBox", "Group", "Revision", "Sdt"];

    /// A catch-all match arm, in the spellings rustfmt produces.
    const CATCH_ALL: [&str; 3] = ["_ =>", "_=>", ".. =>"];

    /// The files this guard reads. Every `.rs` in this crate's `src/`: a walk added
    /// to a new module would escape a per-file list, so if one is added this array
    /// is what has to be extended, and `the_scan_covers_every_module` fails until it
    /// is.
    fn scanned_files() -> Vec<(&'static str, &'static str)> {
        vec![
            ("lib.rs", include_str!("lib.rs")),
            ("containers.rs", include_str!("containers.rs")),
            ("clone.rs", include_str!("clone.rs")),
            ("references.rs", include_str!("references.rs")),
            ("breaks.rs", include_str!("breaks.rs")),
            ("refusal.rs", include_str!("refusal.rs")),
            ("mint.rs", include_str!("mint.rs")),
            ("protection.rs", include_str!("protection.rs")),
            ("access.rs", include_str!("access.rs")),
        ]
    }

    /// The production half of one source file — everything above its test module —
    /// with line endings normalized.
    ///
    /// `include_str!` hands back the bytes as they sit on disk and this repository
    /// has **no `.gitattributes`**, so a Windows checkout is CRLF and any pattern
    /// spanning a line break silently stops matching there. That is not
    /// hypothetical: `casual-doc-wasm`'s equivalent guard passed on macOS and Linux
    /// and failed the `platform (Windows-x64)` job on its first CI run, on exactly
    /// this `expect`. So the normalization comes first, and
    /// `source_scanning_survives_windows_line_endings` drives the CRLF case red on
    /// every platform rather than only on Windows.
    fn production(name: &str, raw: &str) -> String {
        let text = raw.replace("\r\n", "\n");
        // Two spellings, because a module may keep its tests inline or in a sibling
        // file (`#[cfg(test)] #[path = "x_tests.rs"] mod tests;`). Both are a valid
        // cut; a file with NEITHER is not, and still panics — the marker is what
        // stops this guard reading its own test code and reporting itself.
        let tests_at = text
            .find("\n#[cfg(test)]\nmod tests {")
            .or_else(|| text.find("\n#[cfg(test)]\n#[path = "))
            .unwrap_or_else(|| {
                panic!(
                    "{name} must carry a `#[cfg(test)] mod tests` marker — inline or \
                     `#[path]`-ed to a sibling file — for the scan to cut at; without \
                     the cut this guard reads its own test code and reports itself"
                )
            });
        text[..tests_at].to_owned()
    }

    /// One function found by the scan: its name, its 1-based line, and its text
    /// *including* the comment block immediately above it (so a declaration may sit
    /// in the doc comment, which is where a reader looks for it).
    struct Scanned {
        name: String,
        line: usize,
        text: String,
    }

    /// Every function in one production source half, by a line-wise scan.
    ///
    /// Line-wise on purpose, for the CRLF reason above and because it needs no
    /// parser: a function starts at a line whose first tokens are an optional
    /// visibility/qualifier run followed by `fn NAME`, and ends when brace depth
    /// returns to zero. A nested `fn` is reported in its own right AND is part of
    /// its parent's text, which is what we want: either one may carry the
    /// declaration.
    ///
    /// **Limits, stated** (this is a source scan, not a compiler): it cannot tell a
    /// catch-all over `InlineNode` from a catch-all over another enum in the same
    /// function, so it asks for a delegation or a declaration from both — which is
    /// why `ensure_run_boundary` and `run_at_path_mut` were rewritten as `let`/`if
    /// let` instead of arguing with it. It reads only this crate. It cannot check
    /// that a declaration's reason is *true*; it checks that one was written and
    /// that it names a container. And a declaration attaches to whatever function
    /// follows it, so inserting a new function between a declaration and its own
    /// function moves the declaration to the newcomer — observed while
    /// mutation-testing this guard. The build still fails, but it names the
    /// dispossessed function rather than the new one, so read the diff and not only
    /// the message.
    fn scanned_fns(production: &str) -> Vec<Scanned> {
        let lines: Vec<&str> = production.split('\n').collect();
        let mut out = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let Some(name) = fn_name(line) else {
                continue;
            };
            let mut depth = 0i32;
            let mut started = false;
            let mut end = index;
            for (offset, body) in lines[index..].iter().enumerate() {
                depth += i32::try_from(body.matches('{').count()).unwrap_or(0);
                depth -= i32::try_from(body.matches('}').count()).unwrap_or(0);
                if body.contains('{') {
                    started = true;
                }
                if started && depth <= 0 {
                    end = index + offset;
                    break;
                }
            }
            // Walk back over the attribute and comment block above the signature.
            let mut start = index;
            while start > 0 {
                let above = lines[start - 1].trim_start();
                if above.starts_with("//") || above.starts_with("#[") {
                    start -= 1;
                } else {
                    break;
                }
            }
            out.push(Scanned {
                name,
                line: index + 1,
                text: lines[start..=end].join("\n"),
            });
        }
        out
    }

    /// The function name a signature line declares, if it declares one.
    fn fn_name(line: &str) -> Option<String> {
        let mut rest = line.trim_start();
        for qualifier in [
            "pub(crate) ",
            "pub(super) ",
            "pub ",
            "const ",
            "async ",
            "unsafe ",
        ] {
            while let Some(stripped) = rest.strip_prefix(qualifier) {
                rest = stripped;
            }
        }
        let rest = rest.strip_prefix("fn ")?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    }

    /// Whether one scanned function satisfies the rule, and why not when it does
    /// not. `Ok(false)` means "not a subject": it names no inline variant, or it
    /// carries no catch-all and so the compiler already forced it to decide every
    /// variant by hand.
    fn verdict(function: &Scanned) -> Result<bool, String> {
        if !function.text.contains("InlineNode::") {
            return Ok(false);
        }
        // Comment lines are dropped before looking for a catch-all, or the prose
        // *about* `_ =>` in this crate would be read as one — and the rule could
        // then be satisfied by commenting it out.
        let code: String = function
            .text
            .split('\n')
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        if !CATCH_ALL.iter().any(|arm| code.contains(arm)) {
            return Ok(false);
        }
        if CHOKE_POINT.iter().any(|name| function.text.contains(name)) {
            return Ok(true);
        }
        if let Some((_, reason)) = function.text.split_once(DECLARATION) {
            let head: String = reason.chars().take(600).collect();
            if CONTAINERS.iter().any(|container| head.contains(container)) {
                return Ok(true);
            }
            return Err(format!(
                "`{}` declares `{DECLARATION}` but its reason names none of the six \
                 containers, so it does not say what it is declining to enter",
                function.name
            ));
        }
        Err(format!(
            "`{}` walks `InlineNode` behind a catch-all arm without consulting the \
             declared container set (`containers.rs`) and without a \
             `// {DECLARATION} …` line saying why it does not. That is how HF-191, \
             HF-194/195, HF-196 and HF-209 all happened: content plainly on the page \
             that one walk cannot see, decided silently in a wildcard",
            function.name
        ))
    }

    /// Reading this crate's source must not depend on how Git checked it out.
    #[test]
    fn source_scanning_survives_windows_line_endings() {
        for (name, raw) in scanned_files() {
            let lf = raw.replace("\r\n", "\n");
            let crlf = lf.replace('\n', "\r\n");
            assert!(crlf.contains("\r\n"), "{name}: the fixture must be CRLF");
            assert_eq!(
                production(name, &crlf),
                production(name, &lf),
                "{name}: the production slice must be identical however the file was \
                 checked out"
            );
        }
    }

    /// Every module of this crate is in the scan.
    ///
    /// A per-file list is the one way this guard can be escaped without touching
    /// it: add `foo.rs` with a wildcard walk and nothing reads it. So the crate's
    /// own module declarations are counted and compared.
    #[test]
    fn the_scan_covers_every_module() {
        let root = production("lib.rs", include_str!("lib.rs"));
        let mut declared: Vec<String> = root
            .split('\n')
            .filter_map(|line| {
                let line = line.trim_start();
                let rest = line
                    .strip_prefix("pub mod ")
                    .or_else(|| line.strip_prefix("mod "))?;
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (!name.is_empty()).then(|| format!("{name}.rs"))
            })
            .collect();
        declared.push("lib.rs".to_owned());
        declared.sort();
        declared.dedup();
        let mut scanned: Vec<String> = scanned_files()
            .into_iter()
            .map(|(name, _)| name.to_owned())
            .collect();
        scanned.sort();
        assert_eq!(
            declared, scanned,
            "`scanned_files` must list every module of this crate; a module outside \
             the scan is a place the container-set rule is not enforced"
        );
    }

    /// The rule, on this crate: a walk over `InlineNode` either consults the
    /// declared container set or says in one line why it does not (`docs/109`
    /// HF-212).
    ///
    /// This is the deliverable, not the arms. `casual-doc-wasm` fixed nine walks and
    /// this crate's matrix found the same shape in thirty-odd more, because nothing
    /// stopped the next one being written the same way. The mutation that proves it
    /// works is the obvious one: add a walk with a wildcard and no stated reason.
    #[test]
    fn every_inline_walk_consults_the_container_set_or_says_why_not() {
        let mut inline_walks = 0usize;
        let mut failures = Vec::new();
        for (name, raw) in scanned_files() {
            for function in scanned_fns(&production(name, raw)) {
                if function.text.contains("InlineNode::") {
                    inline_walks += 1;
                }
                if let Err(reason) = verdict(&function) {
                    failures.push(format!("{name}:{} {reason}", function.line));
                }
            }
        }
        // The scan finding nothing would pass vacuously, which is the failure mode
        // that makes a guard worse than none. This crate is the editing engine over
        // `v1::InlineNode`; it cannot stop naming it.
        assert!(
            inline_walks >= 30,
            "the scan found only {inline_walks} functions naming `InlineNode::`, so \
             it is not reading this crate and is checking far less than it claims"
        );
        assert!(
            failures.is_empty(),
            "{} walk(s) decide the inline container set silently:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    /// The guard above can fail — proved in-band, on text written here, so the
    /// proof survives the branch that established it.
    ///
    /// A guard whose only evidence is a mutation someone once ran is a guard whose
    /// evidence is gone. Both directions are checked, because a classifier that
    /// rejects everything is as useless as one that accepts everything.
    #[test]
    fn the_container_set_guard_can_fail() {
        let silent = Scanned {
            name: "a_new_walk".to_owned(),
            line: 1,
            text: "fn a_new_walk(inlines: &[InlineNode]) {\n    \
                   for inline in inlines {\n        match inline {\n            \
                   InlineNode::Hyperlink(link) => a_new_walk(&link.inlines),\n            \
                   _ => {}\n        }\n    }\n}"
                .to_owned(),
        };
        let reason = verdict(&silent).expect_err("a wildcard walk with no reason must fail");
        assert!(
            reason.contains("a_new_walk") && reason.contains("container-set"),
            "the failure has to name the walk and the way out: {reason}"
        );

        let delegating = Scanned {
            name: "a_delegating_walk".to_owned(),
            line: 1,
            text: "fn a_delegating_walk(inlines: &[InlineNode]) {\n    \
                   for inline in inlines {\n        match inline {\n            \
                   InlineNode::Drawing(_) => return,\n            _ => {}\n        }\n        \
                   match inline_descent(inline) {\n            \
                   InlineDescent::Inlines(nested) => a_delegating_walk(nested),\n            \
                   InlineDescent::Leaf => {}\n        }\n    }\n}"
                .to_owned(),
        };
        assert_eq!(
            verdict(&delegating),
            Ok(true),
            "consulting the container set must satisfy the rule"
        );

        let declaring = Scanned {
            name: "a_declaring_walk".to_owned(),
            line: 1,
            // container-set: this string is a fixture, not a declaration for the
            // enclosing test — which is itself cut out of the scan. It names
            // `TextBox` so the reason arm is exercised.
            text: "// container-set: top level only; a `TextBox` paragraph's offsets \
                   are not this paragraph's.\nfn a_declaring_walk(inlines: &[InlineNode]) {\n    \
                   for inline in inlines {\n        match inline {\n            \
                   InlineNode::Run(_) => {}\n            _ => {}\n        }\n    }\n}"
                .to_owned(),
        };
        assert_eq!(
            verdict(&declaring),
            Ok(true),
            "a stated reason naming a container must satisfy the rule"
        );

        let hand_waving = Scanned {
            name: "a_hand_waving_walk".to_owned(),
            line: 1,
            text: "// container-set: not applicable.\nfn a_hand_waving_walk(inlines: \
                   &[InlineNode]) {\n    for inline in inlines {\n        match inline {\n \
                   InlineNode::Run(_) => {}\n            _ => {}\n        }\n    }\n}"
                .to_owned(),
        };
        assert!(
            verdict(&hand_waving).is_err(),
            "a declaration that names no container is not a decision"
        );

        // Prose about a catch-all is not a catch-all, or this crate's own comments
        // explaining the rule would violate it — and the rule could be satisfied by
        // commenting the wildcard out.
        let commented = Scanned {
            name: "a_prose_only_walk".to_owned(),
            line: 1,
            text: "fn a_prose_only_walk(inline: &InlineNode) -> bool {\n    \
                   // A `_ =>` arm here would be the defect.\n    \
                   matches!(inline, InlineNode::Run(_))\n}"
                .to_owned(),
        };
        assert_eq!(
            verdict(&commented),
            Ok(false),
            "a function whose only `_ =>` is inside a comment is not a subject"
        );
    }

    /// The function splitter finds the functions it is asked about, including
    /// nested ones — the machinery the rule above rests on.
    #[test]
    fn the_scan_finds_nested_functions_too() {
        let source = "pub fn outer(x: u8) -> u8 {\n    fn inner(y: u8) -> u8 {\n        y\n    \
                      }\n    inner(x)\n}\nconst fn third() -> u8 {\n    0\n}\n";
        let names: Vec<String> = scanned_fns(source)
            .into_iter()
            .map(|function| function.name)
            .collect();
        assert_eq!(names, vec!["outer", "inner", "third"]);
    }

    /// The immutable and mutable faces of the container set classify every inline
    /// kind the same way.
    ///
    /// Rust cannot share one match across mutability, so [`inline_descent`] and
    /// [`inline_descent_mut`] are two spellings of one fact — the exact shape this
    /// module exists to remove. They are kept adjacent so a reader sees both at
    /// once; this fails the build if they ever disagree, which is the part a reader
    /// cannot be relied on for. Drift here is not theoretical: it is how a node gets
    /// FOUND by a reading walk and then silently not changed by a writing one.
    ///
    /// Checked over **every** variant without a fixture, by reading the two matches
    /// themselves: the compiler already guarantees each is exhaustive, so if the two
    /// assign the same axis to the same variant names, they agree everywhere. A
    /// 30th variant therefore arrives for free — `inline_descent` refuses to compile
    /// without a new arm, and this then checks the mutable face got the same one
    /// rather than being quietly swept into `Leaf`.
    #[test]
    fn the_two_faces_of_the_container_set_agree() {
        let source = production("containers.rs", include_str!("containers.rs"));
        let functions = scanned_fns(&source);
        let body_of = |wanted: &str| -> String {
            functions
                .iter()
                .find(|function| function.name == wanted)
                .map(|function| function.text.clone())
                .unwrap_or_else(|| panic!("`{wanted}` must be in this file"))
        };
        let immutable = descent_map(&body_of("inline_descent"));
        let mutable = descent_map(&body_of("inline_descent_mut"));

        assert!(
            immutable.len() >= 25,
            "the parse found only {} arms in `inline_descent`, so this guard is not \
             reading the match and is checking nothing",
            immutable.len()
        );
        assert_eq!(
            immutable, mutable,
            "`inline_descent` and `inline_descent_mut` classify some variant \
             differently: a walk that reads the tree and one that rewrites it would \
             descend different containers, which is how a node gets found by a \
             reading walk and then silently not changed by a writing one"
        );

        // And the declared set is the one this module documents, named here so the
        // six are pinned by the guard and not only by prose.
        for (container, axis) in [
            ("Hyperlink", "Inlines"),
            ("Field", "Inlines"),
            ("Revision", "Inlines"),
            ("Sdt", "Inlines"),
            ("TextBox", "Blocks"),
            ("Group", "Group"),
        ] {
            assert_eq!(
                immutable
                    .iter()
                    .find(|(name, _)| name == container)
                    .map(|(_, axis)| axis.as_str()),
                Some(axis),
                "`{container}` must descend on the {axis} axis"
            );
        }
        let leaves = immutable.iter().filter(|(_, axis)| axis == "Leaf").count();
        assert_eq!(
            leaves,
            immutable.len() - 6,
            "exactly six variants are containers; everything else is a leaf"
        );
    }

    /// `(variant name, axis)` for every arm of a descent match, from its source.
    ///
    /// Line-wise: variant names accumulate across an `|`-separated pattern and are
    /// assigned the axis named on the right of the `=>` that closes it. Comment
    /// lines are skipped, so prose naming a variant is not read as an arm.
    fn descent_map(body: &str) -> Vec<(String, String)> {
        let mut pending: Vec<String> = Vec::new();
        let mut out: Vec<(String, String)> = Vec::new();
        for line in body.split('\n') {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for (index, _) in line.match_indices("InlineNode::") {
                let name: String = line[index + "InlineNode::".len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    pending.push(name);
                }
            }
            let Some((_, right)) = line.split_once("=>") else {
                continue;
            };
            let axis = ["InlineDescentMut::", "InlineDescent::"]
                .iter()
                .find_map(|prefix| right.split_once(prefix))
                .map(|(_, rest)| -> String {
                    rest.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect()
                });
            let Some(axis) = axis else {
                continue;
            };
            for name in pending.drain(..) {
                out.push((name, axis.clone()));
            }
        }
        out.sort();
        out
    }
}
