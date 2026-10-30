// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** A paragraph, with POI's {@code XWPFParagraph} naming. */
public final class OpenParagraph {
    private final OpenDocument document;
    private final String id;
    /** Runs created through {@link #createRun()}, in order, so later runs move when an earlier one grows. */
    private final List<OpenRun> created = new ArrayList<>();

    OpenParagraph(OpenDocument document, String id) {
        this.document = document;
        this.id = id;
    }

    /** The stable node id. */
    public String getId() {
        return id;
    }

    OpenDocument document() {
        return document;
    }

    Map<String, Object> view() {
        return document.view(id);
    }

    /** The paragraph's text ({@code getText}). */
    public String getText() {
        return (String) view().get("text");
    }

    /** The paragraph style's display name ({@code getStyle}). */
    public String getStyle() {
        return (String) view().get("style");
    }

    /** Applies a paragraph style by name or id ({@code setStyle}). */
    public void setStyle(String style) {
        document.edit(OpenDocument.cmd("op", "setParagraphStyle", "paragraph", id, "style", style));
    }

    /** Sets the alignment ({@code setAlignment(ParagraphAlignment)}). */
    public void setAlignment(ParagraphAlignment alignment) {
        document.edit(OpenDocument.cmd("op", "setAlignment", "paragraph", id, "alignment", alignment.value()));
    }

    /** The heading level (0 Title, 1-9), or -1 for body text. */
    public int getHeadingLevel() {
        Object level = view().get("headingLevel");
        return level == null ? -1 : ((Number) level).intValue();
    }

    /** The runs as the engine sees them now ({@code getRuns}). */
    @SuppressWarnings("unchecked")
    public List<OpenRun> getRuns() {
        List<OpenRun> out = new ArrayList<>();
        int offset = 0;
        for (Object raw : (List<Object>) view().get("runs")) {
            Map<String, Object> run = (Map<String, Object>) raw;
            int length = ((String) run.get("text")).codePointCount(0, ((String) run.get("text")).length());
            out.add(new OpenRun(this, offset, length, run));
            offset += length;
        }
        return out;
    }

    /**
     * Appends a run ({@code createRun}). Set its formatting before or after its
     * text, as in POI: {@code r.setBold(true); r.setText("x")}.
     */
    public OpenRun createRun() {
        String text = getText();
        OpenRun run = new OpenRun(this, text.codePointCount(0, text.length()), 0, null);
        created.add(run);
        return run;
    }

    /** Shifts the runs created after {@code run} by {@code delta} characters. */
    void grew(OpenRun run, int delta) {
        boolean after = false;
        for (OpenRun other : created) {
            if (after) {
                other.shift(delta);
            }
            if (other == run) {
                after = true;
            }
        }
    }

    /** Inserts a paragraph before this one ({@code insertNewParagraph} at this position). */
    public OpenParagraph insertParagraphBefore(String text) {
        String newId = (String) document.edit(OpenDocument.cmd("op", "insertParagraph", "before", id, "text", text)).get(0);
        return document.paragraph(newId);
    }

    @Override
    public String toString() {
        return getText();
    }
}
