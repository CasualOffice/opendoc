// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A section's page setup. POI exposes this only as the raw {@code CTSectPr};
 * the getters and setters here are the fields of Word's Page Setup, in points,
 * each setter one engine edit. Sections are addressed by index in body order.
 */
public final class OpenSection {
    private final OpenDocument document;
    private final int index;

    OpenSection(OpenDocument document, int index) {
        this.document = document;
        this.index = index;
    }

    /** The section's index, 0-based in body order. */
    public int getIndex() {
        return index;
    }

    @SuppressWarnings("unchecked")
    private Map<String, Object> raw() {
        return (Map<String, Object>) ((List<Object>) document.call("sections", null)).get(index);
    }

    @SuppressWarnings("unchecked")
    private double margin(String key) {
        Object value = ((Map<String, Object>) raw().get("marginsPt")).get(key);
        return value == null ? 0 : ((Number) value).doubleValue();
    }

    private void setup(Object... pairs) {
        Map<String, Object> setup = new LinkedHashMap<>();
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            setup.put((String) pairs[i], pairs[i + 1]);
        }
        document.edit(OpenDocument.cmd("op", "setPageSetup", "section", index, "setup", setup));
    }

    public double getPageWidth() {
        return ((Number) raw().get("pageWidthPt")).doubleValue();
    }

    public double getPageHeight() {
        return ((Number) raw().get("pageHeightPt")).doubleValue();
    }

    /** Sets the page size in points. */
    public void setPageSize(double widthPt, double heightPt) {
        setup("widthPt", widthPt, "heightPt", heightPt);
    }

    /** {@code "portrait"} or {@code "landscape"}. */
    public String getOrientation() {
        return (String) raw().get("orientation");
    }

    /** Turns the page; the width and height swap, as Word does. */
    public void setOrientation(String orientation) {
        setup("orientation", orientation);
    }

    public double getTopMargin() {
        return margin("top");
    }

    public double getBottomMargin() {
        return margin("bottom");
    }

    public double getLeftMargin() {
        return margin("left");
    }

    public double getRightMargin() {
        return margin("right");
    }

    /** Sets the four margins, in points. */
    public void setMargins(double topPt, double bottomPt, double leftPt, double rightPt) {
        setup("topPt", topPt, "bottomPt", bottomPt, "leftPt", leftPt, "rightPt", rightPt);
    }

    public int getColumns() {
        return ((Number) raw().get("columns")).intValue();
    }

    /** Sets the text columns and the space between them, in points. */
    public void setColumns(int count, double spacingPt) {
        setup("columns", count, "columnSpacingPt", spacingPt);
    }

    public SectionStart getStart() {
        return SectionStart.of((String) raw().get("start"));
    }

    /** Changes how the section starts. */
    public void setStart(SectionStart start) {
        document.edit(OpenDocument.cmd("op", "setSectionStart", "section", index, "start", start.value()));
    }

    /** Whether the first page has its own header and footer. */
    public boolean getTitlePage() {
        return Boolean.TRUE.equals(raw().get("titlePage"));
    }

    public void setTitlePage(boolean on) {
        document.edit(OpenDocument.cmd("op", "setDifferentFirstPage", "section", index, "on", on));
    }

    /** This section's header of {@code type}, created on first use ({@code createHeader}). */
    public OpenHeaderFooter getHeader(HeaderFooterType type) {
        return document.createHeader(index, type);
    }

    /** This section's footer of {@code type}, created on first use ({@code createFooter}). */
    public OpenHeaderFooter getFooter(HeaderFooterType type) {
        return document.createFooter(index, type);
    }
}
