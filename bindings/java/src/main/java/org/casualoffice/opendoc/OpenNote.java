// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.Map;

/**
 * A footnote or endnote ({@code XWPFFootnote}/{@code XWPFEndnote} in POI):
 * {@code getText}, plus {@code setText} and {@code remove}, each one engine
 * edit. Created with {@link OpenParagraph#addFootnote} or
 * {@link OpenParagraph#addEndnote}, which place the reference mark too.
 */
public final class OpenNote {
    private final OpenDocument document;
    private final String id;

    OpenNote(OpenDocument document, String id) {
        this.document = document;
        this.id = id;
    }

    /** The stable note id. */
    public String getId() {
        return id;
    }

    private Map<String, Object> raw() {
        for (Map<String, Object> note : document.getNotes()) {
            if (id.equals(note.get("id"))) {
                return note;
            }
        }
        throw new OpenDocException("invalid-argument", "note " + id + " is no longer in the document");
    }

    /** {@code "footnote"} or {@code "endnote"}. */
    public String getKind() {
        return (String) raw().get("kind");
    }

    /** The note's text. */
    public String getText() {
        return (String) raw().get("text");
    }

    /** Replaces the note's text. */
    public void setText(String text) {
        document.edit(OpenDocument.cmd("op", "setNoteText", "note", id, "text", text));
    }

    /** Removes the note and its reference mark. */
    public void remove() {
        document.edit(OpenDocument.cmd("op", "deleteNote", "note", id));
    }
}
