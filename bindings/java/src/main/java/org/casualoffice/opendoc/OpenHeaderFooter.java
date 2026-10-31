// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * A header or footer, with POI's {@code XWPFHeader}/{@code XWPFFooter} naming:
 * {@code getParagraphs}, {@code createParagraph}, {@code getText}. Its
 * paragraphs are ordinary {@link OpenParagraph}s: runs, formatting and fields
 * ({@link OpenParagraph#addField}) work in them as in the body.
 */
public final class OpenHeaderFooter {
    private final OpenDocument document;
    private final int section;
    private final String region;
    private final HeaderFooterType type;

    OpenHeaderFooter(OpenDocument document, int section, String region, HeaderFooterType type) {
        this.document = document;
        this.section = section;
        this.region = region;
        this.type = type;
    }

    @SuppressWarnings("unchecked")
    private Map<String, Object> story() {
        Map<String, Object> raw = (Map<String, Object>) ((List<Object>) document.call("sections", null)).get(section);
        String wanted = null;
        for (Object ref : (List<Object>) raw.get("header".equals(region) ? "headers" : "footers")) {
            Map<String, Object> r = (Map<String, Object>) ref;
            if (type.value().equals(r.get("kind"))) {
                wanted = (String) r.get("id");
            }
        }
        for (Map<String, Object> story : document.getHeadersFooters()) {
            if (story.get("id").equals(wanted) && region.equals(story.get("part"))) {
                return story;
            }
        }
        // Created on demand: this object stands for the band even before it exists.
        document.edit(OpenDocument.cmd("op", "headerFooter", "section", section, "region", region, "kind", type.value()));
        return story();
    }

    /** The band's paragraphs ({@code getParagraphs}). */
    @SuppressWarnings("unchecked")
    public List<OpenParagraph> getParagraphs() {
        List<OpenParagraph> out = new ArrayList<>();
        for (Object block : (List<Object>) story().get("blocks")) {
            Map<String, Object> b = (Map<String, Object>) block;
            if ("paragraph".equals(b.get("type"))) {
                out.add(document.paragraph((String) b.get("id")));
            }
        }
        return out;
    }

    /** Appends a paragraph ({@code createParagraph}). The band's first, empty paragraph is reused once. */
    public OpenParagraph createParagraph() {
        List<OpenParagraph> paragraphs = getParagraphs();
        OpenParagraph last = paragraphs.get(paragraphs.size() - 1);
        if (paragraphs.size() == 1 && last.getText().isEmpty() && !last.claimed) {
            last.claimed = true;
            return last;
        }
        String id = (String) document.edit(OpenDocument.cmd("op", "insertParagraph", "after", last.getId(), "text", "")).get(0);
        return document.paragraph(id);
    }

    /** The band's text ({@code getText}). */
    public String getText() {
        return (String) story().get("text");
    }

    /** Removes the band: the section shows none of this type. */
    public void remove() {
        document.edit(OpenDocument.cmd("op", "removeHeaderFooter", "section", section, "region", region, "kind", type.value()));
    }
}
