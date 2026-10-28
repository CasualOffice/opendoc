//! The tagged-PDF structure tree (ISO 32000-1 §14.7), flattened into reading
//! order for reflow (`docs/182` §4.2, §5.3: "struct tree first").
//!
//! A structure element (`/StructElem`) names a role (`/S`, mapped to a standard
//! role through the tree's `/RoleMap`), the page it sits on (`/Pg`, inherited by
//! its kids) and its kids (`/K`): marked-content ids (an integer or an `/MCR`
//! dictionary), object references (`/OBJR`, an annotation) or more elements.
//! A depth-first walk of `/K` *is* the logical reading order (§14.7.2), so the
//! flattened leaves are in reading order by construction.
//!
//! Evidence only (PDFR-I1): the reader reports what the producer tagged; the
//! importer decides what it means.
//!
//! Complexity: O(elements + kids), each visited once (a visited set refuses
//! cycles), bounded by `MAX_STRUCT_ELEMENTS` and `MAX_STRUCT_DEPTH`.

use std::collections::{BTreeMap, BTreeSet};

use hayro_syntax::object::{Array, Dict, Name, Object, ObjectIdentifier};

use crate::PdfSource;

/// Struct elements walked before the tree is reported truncated.
pub const MAX_STRUCT_ELEMENTS: usize = 200_000;
/// Nesting depth walked before a subtree is reported truncated.
pub const MAX_STRUCT_DEPTH: usize = 64;
/// `RoleMap` hops followed before a role is taken as written.
const MAX_ROLE_HOPS: usize = 8;

/// One structure element on a leaf's lineage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructNode {
    /// The standard role after the `RoleMap` (`H1`, `P`, `LI`, `TD`, ...), or
    /// the role as written when it maps to nothing.
    pub role: String,
    /// The element's ordinal in walk order: two leaves under the same element
    /// share it, which is how a paragraph tagged in several marked-content
    /// sequences is put back together.
    pub ordinal: u32,
}

/// One marked-content sequence the tree places, in reading order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructLeaf {
    /// Zero-based page index.
    pub page: usize,
    /// The marked-content id on that page.
    pub mcid: i32,
    /// The elements from the root's child down to the one that owns the
    /// sequence (the last entry), so the nearest role is `lineage.last()`.
    pub lineage: Vec<StructNode>,
}

impl StructLeaf {
    /// The nearest element's role.
    #[must_use]
    pub fn role(&self) -> &str {
        self.lineage.last().map_or("", |node| node.role.as_str())
    }

    /// The nearest element's ordinal.
    #[must_use]
    pub fn element(&self) -> u32 {
        self.lineage.last().map_or(0, |node| node.ordinal)
    }
}

/// A flattened structure tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StructureTree {
    /// Marked-content leaves in reading order.
    pub leaves: Vec<StructLeaf>,
    /// Elements walked.
    pub elements: usize,
    /// Whether a bound stopped the walk, so the order is a prefix.
    pub truncated: bool,
}

struct Walk<'m> {
    pages: &'m BTreeMap<ObjectIdentifier, usize>,
    role_map: Option<Dict<'m>>,
    visited: BTreeSet<ObjectIdentifier>,
    tree: StructureTree,
    lineage: Vec<StructNode>,
}

impl Walk<'_> {
    fn standard_role(&self, role: &str) -> String {
        let mut current = role.to_owned();
        if let Some(map) = &self.role_map {
            for _ in 0..MAX_ROLE_HOPS {
                match map.get::<Name<'_>>(current.as_bytes()) {
                    Some(next) if next.as_str() != current => current = next.as_str().to_owned(),
                    _ => break,
                }
            }
        }
        current
    }

    fn page_of(&self, dict: &Dict<'_>) -> Option<usize> {
        dict.get_ref(b"Pg")
            .and_then(|r| self.pages.get(&ObjectIdentifier::from(r)).copied())
    }

    fn leaf(&mut self, page: Option<usize>, mcid: i32) {
        if let Some(page) = page {
            self.tree.leaves.push(StructLeaf {
                page,
                mcid,
                lineage: self.lineage.clone(),
            });
        }
    }

    /// Visits one `/K` entry with the inherited page.
    fn kid(&mut self, kid: &Object<'_>, page: Option<usize>) {
        if self.tree.truncated {
            return;
        }
        match kid {
            Object::Number(n) => self.leaf(page, n.as_i64() as i32),
            Object::Array(array) => self.kids(array, page),
            Object::Dict(dict) => {
                let is = |key: &[u8], value: &[u8]| {
                    dict.get::<Name<'_>>(key)
                        .is_some_and(|name| name.as_str().as_bytes() == value)
                };
                if is(b"Type", b"MCR") {
                    if let Some(mcid) = dict.get::<i32>(b"MCID")
                        && !dict.contains_key(b"Stm")
                    {
                        // A sequence inside a form XObject (`/Stm`) has no
                        // page-level MCID the device sees; it is skipped.
                        let page = self.page_of(dict).or(page);
                        self.leaf(page, mcid);
                    }
                } else if is(b"Type", b"OBJR") {
                    // An annotation or XObject placed in the order; the
                    // importer reads annotations from the page itself.
                } else if dict.contains_key(b"S") {
                    self.element(dict, page);
                }
            }
            _ => {}
        }
    }

    fn kids(&mut self, array: &Array<'_>, page: Option<usize>) {
        for kid in array.iter::<Object<'_>>() {
            if self.tree.truncated {
                return;
            }
            self.kid(&kid, page);
        }
    }

    fn element(&mut self, dict: &Dict<'_>, page: Option<usize>) {
        if let Some(id) = dict.obj_id()
            && !self.visited.insert(id)
        {
            return;
        }
        if self.tree.elements >= MAX_STRUCT_ELEMENTS || self.lineage.len() >= MAX_STRUCT_DEPTH {
            self.tree.truncated = true;
            return;
        }
        let ordinal = u32::try_from(self.tree.elements).unwrap_or(u32::MAX);
        self.tree.elements += 1;
        let role = dict
            .get::<Name<'_>>(b"S")
            .map(|name| self.standard_role(name.as_str()))
            .unwrap_or_default();
        let page = self.page_of(dict).or(page);
        self.lineage.push(StructNode { role, ordinal });
        if let Some(kids) = dict.get::<Object<'_>>(b"K") {
            self.kid(&kids, page);
        }
        self.lineage.pop();
    }
}

impl PdfSource {
    /// The document's structure tree flattened into reading order, or `None`
    /// for an untagged file (no `/StructTreeRoot`). O(elements), bounded.
    #[must_use]
    pub fn structure_tree(&self) -> Option<StructureTree> {
        let xref = self.pdf.xref();
        let catalog = xref.get::<Dict<'_>>(xref.root_id())?;
        let root = catalog.get::<Dict<'_>>(b"StructTreeRoot")?;
        let pages: BTreeMap<ObjectIdentifier, usize> = self
            .pdf
            .pages()
            .iter()
            .enumerate()
            .filter_map(|(index, page)| page.raw().obj_id().map(|id| (id, index)))
            .collect();
        let mut walk = Walk {
            pages: &pages,
            role_map: root.get::<Dict<'_>>(b"RoleMap"),
            visited: BTreeSet::new(),
            tree: StructureTree::default(),
            lineage: Vec::new(),
        };
        if let Some(kids) = root.get::<Object<'_>>(b"K") {
            walk.kid(&kids, None);
        }
        Some(walk.tree)
    }
}
