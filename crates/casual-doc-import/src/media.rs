//! Media-reference mapping: main-document image relationships -> v1 media
//! references (no image bytes are decoded).

use std::collections::BTreeMap;

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{DefinitionMap, MediaId, MediaReference};

use crate::error::ImportError;
use crate::report::Reporter;

/// One image relationship resolved from the package, before an id is assigned.
#[derive(Clone)]
pub(crate) struct MediaSource {
    pub relationship_id: String,
    pub media_type: String,
    pub part_name: String,
}

/// Builds one part's image relationships into the shared media table, allocating
/// a deterministic id per relationship (no de-duplication, so a part's media —
/// and the main document's — behaves exactly as before this aggregation), and
/// returns that part's relationship-id -> id index. Out-of-domain references are
/// dropped with a report. Each part is called with its own sources, so per-part
/// relationship ids (which collide across parts) resolve independently.
pub(crate) fn build_into(
    sources: &[MediaSource],
    media: &mut DefinitionMap<MediaId, MediaReference>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
) -> Result<BTreeMap<String, MediaId>, ImportError> {
    let mut index = BTreeMap::new();
    for source in sources {
        if !in_domain(&source.relationship_id, 255)
            || !in_domain(&source.media_type, 255)
            || !in_domain(&source.part_name, 1_024)
        {
            reporter.report(b"image");
            continue;
        }
        let id = MediaId::new(
            ids.next_id()
                .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?,
        );
        media.insert(
            id,
            MediaReference {
                relationship_id: source.relationship_id.clone(),
                media_type: source.media_type.clone(),
                part_name: source.part_name.clone(),
            },
        );
        index.insert(source.relationship_id.clone(), id);
    }
    Ok(index)
}

fn in_domain(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}
