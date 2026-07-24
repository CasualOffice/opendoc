//! Media-reference mapping: main-document image relationships -> v1 media
//! references (no image bytes are decoded).

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{DefinitionMap, MediaId, MediaReference};

use crate::error::ImportError;
use crate::report::Reporter;

/// One image relationship resolved from the package, before an id is assigned.
pub(crate) struct MediaSource {
    pub relationship_id: String,
    pub media_type: String,
    pub part_name: String,
}

/// Builds the media definition table, allocating deterministic ids and dropping
/// (with a report) any reference whose fields fall outside the v1 domain.
pub(crate) fn build(
    sources: &[MediaSource],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
) -> Result<DefinitionMap<MediaId, MediaReference>, ImportError> {
    let mut media = DefinitionMap::default();
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
    }
    Ok(media)
}

fn in_domain(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}
