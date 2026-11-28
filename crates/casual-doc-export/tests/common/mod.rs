// SPDX-License-Identifier: Apache-2.0

//! Helpers shared by the integration tests that read a *written* package back.
//!
//! `no_written_part_uses_an_undeclared_prefix` started life inside
//! `collapsed_heading_state.rs` and is the only guard that walks every written XML
//! part with a namespace-aware reader. Moving the walk here lets a second test
//! file point the same guard at another package instead of copying it, which is
//! what a drifting copy would eventually stop agreeing with.

// Each test crate that declares `mod common;` uses a subset of what is here, and
// rustc reports the rest as dead code per crate; that is the idiom for a shared
// test module, not an unused helper.
#![allow(dead_code)]

pub mod story_pictures;

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

/// The `.xml` and `.rels` parts of a package, keyed by name.
pub fn xml_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("a valid ZIP entry name").into_owned())
        .collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        parts.insert(name, buffer);
    }
    parts
}

/// Walks every XML part of `package` with a namespace-aware reader and panics if
/// one is malformed or uses an element or attribute prefix its root does not
/// declare. Returns how many elements it checked, so a caller can refuse a vacuous
/// pass.
///
/// `label` names the package in the failure message.
pub fn assert_no_part_uses_an_undeclared_prefix(label: &str, package: &[u8]) -> usize {
    let mut checked = 0usize;
    for (part, bytes) in xml_parts(package) {
        let mut reader = quick_xml::NsReader::from_reader(bytes.as_slice());
        let mut buffer = Vec::new();
        loop {
            match reader.read_resolved_event_into(&mut buffer) {
                Ok((_, Event::Eof)) => break,
                Ok((resolved, event)) => {
                    if let Event::Start(ref element) | Event::Empty(ref element) = event {
                        assert!(
                            !matches!(resolved, ResolveResult::Unknown(_)),
                            "{label}: part `{part}` uses an XML prefix its root does not \
                             declare — Word refuses such a part, and nothing else in \
                             this suite would notice"
                        );
                        for attribute in element.attributes() {
                            let attribute = attribute.unwrap_or_else(|error| {
                                panic!("{label}: part `{part}` has a bad attribute: {error}")
                            });
                            let (resolved, _) = reader.resolver().resolve_attribute(attribute.key);
                            assert!(
                                !matches!(resolved, ResolveResult::Unknown(_)),
                                "{label}: part `{part}` has an attribute whose prefix its \
                                 root does not declare"
                            );
                        }
                        checked += 1;
                    }
                }
                Err(error) => panic!("{label}: part `{part}` must be well-formed XML: {error}"),
            }
            buffer.clear();
        }
    }
    checked
}
