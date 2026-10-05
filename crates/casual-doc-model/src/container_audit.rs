// SPDX-License-Identifier: Apache-2.0

//! The build guard that holds every crate to the declared inline container set
//! (`docs/109` HF-212).
//!
//! # Why this is public API of the model crate
//!
//! [`v1::inline_descent`](crate::v1::inline_descent) declares what an
//! `InlineNode` contains. A declaration nothing enforces is a comment, and the
//! defect HF-212 names is precisely that each walk decided the set for itself
//! inside a `_ =>` arm — so the rule and the declaration have to ship together,
//! in the crate that owns the type.
//!
//! The alternative was a copy of this scanner in every crate that walks an
//! inline, which is the same mistake one level up: `casual-doc-wasm` and
//! `casual-doc-edit` each closed this class with their own copy of the six-arm
//! match, and the second copy's own header reported the duplication as a row.
//! `SKILL` §8: *prefer one mechanism over two.* So the scanner is here, public,
//! and each crate arms it over its own source with four lines of test:
//!
//! ```no_run
//! use casual_doc_model::container_audit::{Audit, SourceFile};
//!
//! let files = vec![SourceFile::new("lib.rs", include_str!("lib.rs"))];
//! let audit = Audit::new(files).expecting_at_least(1);
//! audit.assert_covers_declared_modules();
//! audit.run().assert_clean();
//! ```
//!
//! # What it checks, and what it cannot
//!
//! A function that names `InlineNode::` **and** carries a catch-all match arm
//! must either consult the declared set (by naming one of the choke-point
//! functions) or carry a one-line `// container-set: …` declaration whose reason
//! names at least one of the six containers. A function with no catch-all is not
//! a subject: the compiler already forced it to decide every variant by hand.
//!
//! **Limits, stated** (this is a source scan, not a compiler): it cannot tell a
//! catch-all over `InlineNode` from a catch-all over another enum in the same
//! function, so it asks for a delegation or a declaration from both. It reads
//! only the files it is handed. It cannot check that a declaration's reason is
//! *true*; it checks that one was written and that it names a container. And a
//! declaration attaches to whatever function follows it, so inserting a new
//! function between a declaration and its own function moves the declaration to
//! the newcomer — observed while mutation-testing this guard. The build still
//! fails, but it names the dispossessed function rather than the new one, so read
//! the diff and not only the message.

/// The complete inline container set. A `container-set:` declaration has to name
/// at least one of these, so the author engages with the set rather than writing
/// "n/a".
pub const CONTAINERS: [&str; 6] = ["Hyperlink", "Field", "TextBox", "Group", "Revision", "Sdt"];

/// The names that count as consulting the declared container set. A walk that
/// mentions one of these gets its descent from
/// [`v1::containers`](crate::v1::inline_descent) and therefore cannot be missing a
/// container.
const CHOKE_POINT: [&str; 3] = [
    "inline_descent",
    "contained_inlines",
    "find_in_group_block_stories",
];

/// The one-line escape hatch marker, for a walk that deliberately does not need
/// the container set.
const DECLARATION: &str = "container-set:";

/// A catch-all match arm, in the spellings rustfmt produces.
const CATCH_ALL: [&str; 3] = ["_ =>", "_=>", ".. =>"];

/// One source file to audit: its display name and its text.
///
/// The text comes from `include_str!` at the call site, because that is the only
/// way a crate can hand its own source to a library: a path would depend on the
/// working directory a test happens to run in.
#[derive(Clone, Copy, Debug)]
pub struct SourceFile<'a> {
    name: &'a str,
    text: &'a str,
}

impl<'a> SourceFile<'a> {
    /// A file to audit.
    #[must_use]
    pub const fn new(name: &'a str, text: &'a str) -> Self {
        Self { name, text }
    }

    /// The display name this file is reported under.
    #[must_use]
    pub const fn name(&self) -> &'a str {
        self.name
    }
}

/// One crate's arming of the container-set rule.
#[derive(Clone, Debug)]
pub struct Audit<'a> {
    files: Vec<SourceFile<'a>>,
    also_consulting: Vec<&'a str>,
    minimum_walks: usize,
}

impl<'a> Audit<'a> {
    /// An audit over `files`.
    #[must_use]
    pub fn new(files: Vec<SourceFile<'a>>) -> Self {
        Self {
            files,
            also_consulting: Vec::new(),
            minimum_walks: 1,
        }
    }

    /// Additional function names that count as consulting the declared set.
    ///
    /// For a crate-local *axis of* the declaration — a helper whose children come
    /// from [`contained_inlines`](crate::v1::contained_inlines) and whose own
    /// contribution is only policy — rather than for a second copy of it.
    #[must_use]
    pub fn also_consulting(mut self, names: &[&'a str]) -> Self {
        self.also_consulting.extend_from_slice(names);
        self
    }

    /// How many functions mentioning `InlineNode` this scan must find.
    ///
    /// A scan that finds nothing passes vacuously, which is the failure mode that
    /// makes a guard worse than none, so each crate states the floor it knows it
    /// is above.
    ///
    /// The floor counts every function that MENTIONS the type, not only the ones
    /// that are subjects of the rule, because fixing a walk removes `InlineNode::`
    /// from it — it delegates instead — and a floor that fell every time the rule
    /// was obeyed would have to be edited downwards to stay green. That is a
    /// ratchet pointing the wrong way.
    #[must_use]
    pub const fn expecting_at_least(mut self, walks: usize) -> Self {
        self.minimum_walks = walks;
        self
    }

    /// Fails unless every module the audited files declare is itself audited.
    ///
    /// A per-file list is the one way this guard can be escaped without touching
    /// it: add `foo.rs` with a wildcard walk and nothing reads it. So the
    /// declarations in the audited sources are counted and compared against the
    /// scan, by file basename.
    ///
    /// # Panics
    ///
    /// When a declared module is missing from the audit.
    pub fn assert_covers_declared_modules(&self) {
        let mut declared: Vec<String> = Vec::new();
        for file in &self.files {
            declared.extend(declared_modules(&production_half(file.text)));
        }
        declared.sort();
        declared.dedup();
        let mut scanned: Vec<String> = self
            .files
            .iter()
            .map(|file| basename(file.name).to_owned())
            .collect();
        scanned.sort();
        scanned.dedup();
        let missing: Vec<&String> = declared
            .iter()
            .filter(|module| !scanned.contains(module))
            .collect();
        assert!(
            missing.is_empty(),
            "the audit must cover every module these sources declare; {missing:?} \
             {} outside it, and a module outside the scan is a place the \
             container-set rule is not enforced",
            if missing.len() == 1 { "is" } else { "are" }
        );
    }

    /// Runs the audit.
    #[must_use]
    pub fn run(&self) -> Outcome {
        let mut inline_walks = 0usize;
        let mut failures = Vec::new();
        for file in &self.files {
            for function in scanned_functions(&production_half(file.text)) {
                if function.text.contains("InlineNode") {
                    inline_walks += 1;
                }
                if let Err(reason) = self.verdict(&function) {
                    failures.push(format!("{}:{} {reason}", file.name, function.line));
                }
            }
        }
        Outcome {
            inline_walks,
            minimum_walks: self.minimum_walks,
            failures,
        }
    }

    /// Whether one scanned function satisfies the rule, and why not when it does
    /// not. `Ok(false)` means "not a subject": it names no inline variant, or it
    /// carries no catch-all and so the compiler already forced it to decide every
    /// variant by hand.
    fn verdict(&self, function: &ScannedFunction) -> Result<bool, String> {
        if !function.text.contains("InlineNode::") {
            return Ok(false);
        }
        // Comment lines are dropped before looking for a catch-all, or the prose
        // *about* `_ =>` in a crate that explains the rule would be read as one —
        // and the rule could then be satisfied by commenting the wildcard out.
        let code: String = function
            .text
            .split('\n')
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        if !CATCH_ALL.iter().any(|arm| code.contains(arm)) {
            return Ok(false);
        }
        if CHOKE_POINT
            .iter()
            .chain(self.also_consulting.iter())
            .any(|name| function.text.contains(name))
        {
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
             declared container set (`casual_doc_model::v1::inline_descent`) and \
             without a `// {DECLARATION} …` line saying why it does not. That is how \
             HF-191, HF-194/195, HF-196 and HF-209 all happened: content plainly on \
             the page that one walk cannot see, decided silently in a wildcard",
            function.name
        ))
    }
}

/// What one [`Audit`] found.
#[derive(Clone, Debug)]
pub struct Outcome {
    inline_walks: usize,
    minimum_walks: usize,
    failures: Vec<String>,
}

impl Outcome {
    /// How many functions mentioning `InlineNode` the scan read.
    #[must_use]
    pub const fn inline_walks(&self) -> usize {
        self.inline_walks
    }

    /// One line per walk that decides the container set silently.
    #[must_use]
    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    /// Fails the build unless the scan read what it claims to and found nothing.
    ///
    /// # Panics
    ///
    /// When the scan read fewer walks than the crate declared, or when any walk
    /// decides the container set silently.
    pub fn assert_clean(&self) {
        assert!(
            self.inline_walks >= self.minimum_walks,
            "the scan found only {} functions mentioning `InlineNode` where this \
             crate declared at least {}, so it is not reading the source it was \
             handed and is checking far less than it claims",
            self.inline_walks,
            self.minimum_walks
        );
        assert!(
            self.failures.is_empty(),
            "{} walk(s) decide the inline container set silently:\n  {}",
            self.failures.len(),
            self.failures.join("\n  ")
        );
    }
}

/// The production half of one source file — everything above its test module —
/// with line endings normalized.
///
/// `include_str!` hands back the bytes as they sit on disk and this repository
/// has **no `.gitattributes`**, so a Windows checkout is CRLF and any pattern
/// spanning a line break silently stops matching there. That is not
/// hypothetical: `casual-doc-wasm`'s equivalent guard passed on macOS and Linux
/// and failed the `platform (Windows-x64)` job on its first CI run, on exactly
/// this cut. So the normalization comes first.
///
/// A file with **no** test module is production in its entirety, which is the
/// common case outside the editing crate. The cut exists only so the guard does
/// not read its own test fixtures and report itself; a mid-file `#[cfg(test)]`
/// `use` or helper is deliberately NOT a cut, because cutting there would hide
/// every production walk below it.
#[must_use]
pub fn production_half(raw: &str) -> String {
    let text = raw.replace("\r\n", "\n");
    // Three spellings, because a module may keep its tests inline, in a sibling
    // file (`#[cfg(test)] #[path = "x_tests.rs"] mod tests;`), or as a plain
    // `mod tests;` beside it.
    let tests_at = text
        .find("\n#[cfg(test)]\nmod tests {")
        .or_else(|| text.find("\n#[cfg(test)]\n#[path = "))
        .or_else(|| text.find("\n#[cfg(test)]\nmod tests;"));
    match tests_at {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}

/// The module names one production source half declares, as `<name>.rs`.
#[must_use]
fn declared_modules(production: &str) -> Vec<String> {
    production
        .split('\n')
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line
                .strip_prefix("pub mod ")
                .or_else(|| line.strip_prefix("pub(crate) mod "))
                .or_else(|| line.strip_prefix("mod "))?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then(|| format!("{name}.rs"))
        })
        .collect()
}

/// The last path component of a file name, so `transform/effect.rs` matches the
/// `mod effect;` that declares it.
fn basename(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// One function found by the scan: its name, its 1-based line, and its text
/// *including* the comment block immediately above it (so a declaration may sit
/// in the doc comment, which is where a reader looks for it).
#[derive(Clone, Debug)]
struct ScannedFunction {
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
fn scanned_functions(production: &str) -> Vec<ScannedFunction> {
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
        out.push(ScannedFunction {
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

#[cfg(test)]
mod tests {
    use super::{Audit, ScannedFunction, SourceFile, production_half, scanned_functions};

    fn audit() -> Audit<'static> {
        Audit::new(Vec::new())
    }

    /// Reading a crate's source must not depend on how Git checked it out.
    #[test]
    fn source_scanning_survives_windows_line_endings() {
        for (name, raw) in [
            ("container_audit.rs", include_str!("container_audit.rs")),
            ("containers.rs", include_str!("v1/containers.rs")),
        ] {
            let lf = raw.replace("\r\n", "\n");
            let crlf = lf.replace('\n', "\r\n");
            assert!(crlf.contains("\r\n"), "{name}: the fixture must be CRLF");
            assert_eq!(
                production_half(&crlf),
                production_half(&lf),
                "{name}: the production slice must be identical however the file was \
                 checked out"
            );
        }
    }

    /// A mid-file `#[cfg(test)]` `use` is not a cut.
    ///
    /// Cutting there would hide every production walk below it, which is how a
    /// scan reports itself clean over a file it barely read. `transform.rs` in
    /// `casual-doc-transaction` is exactly this shape: a `#[cfg(test)] use` at
    /// line 713 and 1,270 more lines of production code after it.
    #[test]
    fn a_mid_file_test_only_import_does_not_cut_the_scan() {
        let source = "fn above() {}\n#[cfg(test)]\nuse std::fmt;\nfn below() {}\n\
                      #[cfg(test)]\nmod tests {\n    fn inside() {}\n}\n";
        let production = production_half(source);
        assert!(production.contains("fn below"), "{production}");
        assert!(!production.contains("fn inside"), "{production}");
    }

    /// The guard can fail — proved in-band, on text written here, so the proof
    /// survives the branch that established it.
    ///
    /// A guard whose only evidence is a mutation someone once ran is a guard whose
    /// evidence is gone. Both directions are checked, because a classifier that
    /// rejects everything is as useless as one that accepts everything.
    #[test]
    fn the_container_set_guard_can_fail() {
        let silent = ScannedFunction {
            name: "a_new_walk".to_owned(),
            line: 1,
            text: "fn a_new_walk(inlines: &[InlineNode]) {\n    \
                   for inline in inlines {\n        match inline {\n            \
                   InlineNode::Hyperlink(link) => a_new_walk(&link.inlines),\n            \
                   _ => {}\n        }\n    }\n}"
                .to_owned(),
        };
        let reason = audit()
            .verdict(&silent)
            .expect_err("a wildcard walk with no reason must fail");
        assert!(
            reason.contains("a_new_walk") && reason.contains("container-set"),
            "the failure has to name the walk and the way out: {reason}"
        );

        let delegating = ScannedFunction {
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
            audit().verdict(&delegating),
            Ok(true),
            "consulting the container set must satisfy the rule"
        );

        let declaring = ScannedFunction {
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
            audit().verdict(&declaring),
            Ok(true),
            "a stated reason naming a container must satisfy the rule"
        );

        let hand_waving = ScannedFunction {
            name: "a_hand_waving_walk".to_owned(),
            line: 1,
            text: "// container-set: not applicable.\nfn a_hand_waving_walk(inlines: \
                   &[InlineNode]) {\n    for inline in inlines {\n        match inline {\n \
                   InlineNode::Run(_) => {}\n            _ => {}\n        }\n    }\n}"
                .to_owned(),
        };
        assert!(
            audit().verdict(&hand_waving).is_err(),
            "a declaration that names no container is not a decision"
        );

        // Prose about a catch-all is not a catch-all, or a crate's own comments
        // explaining the rule would violate it — and the rule could be satisfied by
        // commenting the wildcard out.
        let commented = ScannedFunction {
            name: "a_prose_only_walk".to_owned(),
            line: 1,
            text: "fn a_prose_only_walk(inline: &InlineNode) -> bool {\n    \
                   // A `_ =>` arm here would be the defect.\n    \
                   matches!(inline, InlineNode::Run(_))\n}"
                .to_owned(),
        };
        assert_eq!(
            audit().verdict(&commented),
            Ok(false),
            "a function whose only `_ =>` is inside a comment is not a subject"
        );

        // A crate-local axis of the declaration counts only when the crate says so.
        let local_axis = ScannedFunction {
            name: "a_local_axis_walk".to_owned(),
            line: 1,
            text: "fn a_local_axis_walk(inline: &InlineNode) -> bool {\n    \
                   match inline {\n        InlineNode::Run(_) => true,\n        \
                   _ => editing_transparent(inline),\n    }\n}"
                .to_owned(),
        };
        assert!(
            audit().verdict(&local_axis).is_err(),
            "an unregistered helper must not satisfy the rule"
        );
        assert_eq!(
            audit()
                .also_consulting(&["editing_transparent"])
                .verdict(&local_axis),
            Ok(true),
            "a registered crate-local axis must satisfy it"
        );
    }

    /// The function splitter finds the functions it is asked about, including
    /// nested ones — the machinery the rule above rests on.
    #[test]
    fn the_scan_finds_nested_functions_too() {
        let source = "pub fn outer(x: u8) -> u8 {\n    fn inner(y: u8) -> u8 {\n        y\n    \
                      }\n    inner(x)\n}\nconst fn third() -> u8 {\n    0\n}\n";
        let names: Vec<String> = scanned_functions(source)
            .into_iter()
            .map(|function| function.name)
            .collect();
        assert_eq!(names, vec!["outer", "inner", "third"]);
    }

    /// A scan that read nothing must not pass.
    #[test]
    fn an_empty_scan_is_not_a_pass() {
        let outcome = Audit::new(vec![SourceFile::new("empty.rs", "fn nothing() {}\n")])
            .expecting_at_least(1)
            .run();
        assert_eq!(outcome.inline_walks(), 0);
        let failure = std::panic::catch_unwind(move || outcome.assert_clean())
            .expect_err("a vacuous scan must fail");
        let message = failure
            .downcast_ref::<String>()
            .map_or_else(String::new, Clone::clone);
        assert!(
            message.contains("checking far less than it claims"),
            "{message}"
        );
    }

    /// The module-coverage check names the module that escaped the scan.
    #[test]
    fn a_module_outside_the_scan_fails_the_coverage_check() {
        let audit = Audit::new(vec![SourceFile::new("lib.rs", "mod hidden;\nmod seen;\n")]);
        let failure = std::panic::catch_unwind(move || audit.assert_covers_declared_modules())
            .expect_err("an unaudited module must fail");
        let message = failure
            .downcast_ref::<String>()
            .map_or_else(String::new, Clone::clone);
        assert!(message.contains("hidden.rs"), "{message}");
    }

    /// A module declared in a nested file is matched by basename.
    #[test]
    fn a_nested_module_path_satisfies_the_coverage_check() {
        Audit::new(vec![
            SourceFile::new("transform.rs", "mod effect;\n"),
            SourceFile::new("transform/effect.rs", "fn nothing() {}\n"),
        ])
        .assert_covers_declared_modules();
    }
}
