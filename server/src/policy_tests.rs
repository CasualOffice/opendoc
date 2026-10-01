// SPDX-License-Identifier: Apache-2.0

//! The structural guard: **no mandatory server.**

/// Nothing under `crates/` may depend on the relay.
///
/// Read from the **manifests**, not the source, because a `path` dependency in a `Cargo.toml` is
/// how this would actually happen — one `use` later nothing in the engine would compile without a
/// relay in the tree, and "no mandatory server" would have become false without a single line of
/// prose changing.
///
/// This is the same shape as `the_live_editor_has_no_collaboration_dependency` one level up: that
/// one stops the *editor* reaching collaboration, this one stops the *engine* reaching a server.
#[test]
fn nothing_under_crates_depends_on_the_relay() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("crates");
    let mut offenders = Vec::new();
    let entries = std::fs::read_dir(&crates).expect("the crates directory is there");
    let mut seen = 0_usize;
    for entry in entries {
        let manifest = entry.expect("a readable entry").path().join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        seen += 1;
        for forbidden in [
            "opendoc-relay",
            "opendoc_relay",
            "path = \"../../server",
            "server/",
        ] {
            if text.contains(forbidden) {
                offenders.push(format!("{} names `{forbidden}`", manifest.display()));
            }
        }
    }
    assert!(
        seen >= 10,
        "only {seen} manifests were read, so this guard was not looking at the workspace"
    );
    assert!(
        offenders.is_empty(),
        "a crate now depends on the relay, which would make the server mandatory: {offenders:?}"
    );
    // The guard must be able to see what it forbids.
    let planted = "opendoc-relay = { path = \"../../server\" }";
    assert!(
        ["opendoc-relay", "path = \"../../server"]
            .iter()
            .any(|item| planted.contains(item)),
        "the scan cannot see the dependency it exists to forbid"
    );
}
