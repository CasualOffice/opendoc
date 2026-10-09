//! Footnotes and endnotes: the reference as the page labels it, the notes
//! written after the body, linked both ways.

use casual_doc_model::v1::{NoteId, NoteKind};

use super::{Cx, Flow, Writer};
use crate::html::escape_text;
use crate::{AdapterError, ModelOutcome};

impl Writer<'_> {
    /// A footnote or endnote reference: the label the page prints, linked to
    /// the note at the end of the file, and linked back from it.
    pub(super) fn note_reference(
        &mut self,
        kind: NoteKind,
        note: NoteId,
    ) -> Result<(), AdapterError> {
        let exists = match kind {
            NoteKind::Footnote => self.definitions.footnotes.get(&note).is_some(),
            NoteKind::Endnote => self.definitions.endnotes.get(&note).is_some(),
        };
        if !exists {
            self.losses
                .record("html.dangling_note", ModelOutcome::Omitted);
            return Ok(());
        }
        let label = self
            .note_labels
            .label(kind, note)
            .map_or_else(|| "*".to_owned(), str::to_owned);
        let (index, first) = match self.notes.iter().position(|seen| *seen == (kind, note)) {
            Some(index) => (index, false),
            None => {
                self.notes.push((kind, note));
                (self.notes.len() - 1, true)
            }
        };
        let id = note_anchor(kind, index);
        self.push("<sup class=\"note-ref\"><a href=\"#")?;
        self.push(&id)?;
        self.push("\"")?;
        if first {
            self.push(" id=\"ref-")?;
            self.push(&id)?;
            self.push("\"")?;
        }
        self.push(">")?;
        self.push(&escape_text(&label))?;
        self.push("</a></sup>")
    }

    /// The number at the head of a note's own text, linking back to where the
    /// note is referenced.
    pub(super) fn note_number_mark(&mut self) -> Result<(), AdapterError> {
        let Some((index, kind)) = self.current_note else {
            return Ok(());
        };
        let (note_kind, note) = self.notes[index];
        let label = self
            .note_labels
            .label(note_kind, note)
            .map_or_else(|| "*".to_owned(), str::to_owned);
        let id = note_anchor(kind, index);
        self.push("<sup class=\"note-ref\"><a class=\"note-back\" href=\"#ref-")?;
        self.push(&id)?;
        self.push("\">")?;
        self.push(&escape_text(&label))?;
        self.push("</a></sup>")
    }

    /// Every referenced note, after the body: footnotes, then endnotes.
    pub(super) fn write_notes(&mut self) -> Result<(), AdapterError> {
        // A note may reference another note; the queue can grow while this
        // walks it, so it is walked by index.
        for kind in [NoteKind::Footnote, NoteKind::Endnote] {
            let mut opened = false;
            let mut index = 0;
            while index < self.notes.len() {
                let (note_kind, note) = self.notes[index];
                if note_kind != kind {
                    index += 1;
                    continue;
                }
                let blocks = match kind {
                    NoteKind::Footnote => self.definitions.footnotes.get(&note),
                    NoteKind::Endnote => self.definitions.endnotes.get(&note),
                }
                .map(|note| note.blocks.clone())
                .unwrap_or_default();
                if !opened {
                    self.push(match kind {
                        NoteKind::Footnote => "<section class=\"notes footnotes\">\n<hr>\n",
                        NoteKind::Endnote => "<section class=\"notes endnotes\">\n<hr>\n",
                    })?;
                    opened = true;
                }
                self.push("<div class=\"note\" id=\"")?;
                self.push(&note_anchor(kind, index))?;
                self.push("\">\n")?;
                self.current_note = Some((index, kind));
                let mut flow = Flow::default();
                let cx = Cx {
                    depth: 1,
                    layer: None,
                };
                self.blocks(&blocks, cx, &mut flow)?;
                self.current_note = None;
                self.push("</div>\n")?;
                index += 1;
            }
            if opened {
                self.push("</section>\n")?;
            }
        }
        Ok(())
    }
}

fn note_anchor(kind: NoteKind, index: usize) -> String {
    match kind {
        NoteKind::Footnote => format!("fn{}", index + 1),
        NoteKind::Endnote => format!("en{}", index + 1),
    }
}
