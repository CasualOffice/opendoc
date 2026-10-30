// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * A run of text, with POI's {@code XWPFRun} naming: {@code setText},
 * {@code setBold}, {@code setItalic}, {@code setUnderline}, {@code setStrike},
 * {@code setFontSize}, {@code setFontFamily}, {@code setColor}. A setter
 * formats exactly the characters the run covers, through one engine edit.
 */
public final class OpenRun {
    private final OpenParagraph paragraph;
    private int start;
    private int length;
    private final Map<String, Object> snapshot;
    /** Formatting set before the run had text; applied when it gets some. */
    private final Map<String, Object> pending = new LinkedHashMap<>();

    OpenRun(OpenParagraph paragraph, int start, int length, Map<String, Object> snapshot) {
        this.paragraph = paragraph;
        this.start = start;
        this.length = length;
        this.snapshot = snapshot;
    }

    void shift(int delta) {
        start += delta;
    }

    private OpenDocument doc() {
        return paragraph.document();
    }

    /** Sets the run's text, replacing what it had ({@code setText}). */
    public void setText(String text) {
        int newLength = text.codePointCount(0, text.length());
        if (length == 0 && newLength > 0 && start == codePoints(paragraph.getText())) {
            doc().edit(OpenDocument.cmd("op", "appendRun", "paragraph", paragraph.getId(), "text", text));
        } else {
            if (length > 0) {
                doc().edit(OpenDocument.cmd("op", "deleteText", "paragraph", paragraph.getId(), "start", start, "end", start + length));
            }
            if (newLength > 0) {
                doc().edit(OpenDocument.cmd("op", "insertText", "paragraph", paragraph.getId(), "offset", start, "text", text));
            }
        }
        paragraph.grew(this, newLength - length);
        length = newLength;
        if (!pending.isEmpty() && length > 0) {
            format(new LinkedHashMap<>(pending));
            pending.clear();
        }
    }

    /** The run's text ({@code getText(0)}). */
    public String getText() {
        String text = paragraph.getText();
        int from = text.offsetByCodePoints(0, Math.min(start, codePoints(text)));
        int to = text.offsetByCodePoints(from, Math.min(length, codePoints(text) - start));
        return text.substring(from, to);
    }

    private static int codePoints(String text) {
        return text.codePointCount(0, text.length());
    }

    private void set(String key, Object value) {
        if (length == 0) {
            pending.put(key, value);
            return;
        }
        Map<String, Object> format = new LinkedHashMap<>();
        format.put(key, value);
        format(format);
    }

    private void format(Map<String, Object> format) {
        doc().edit(OpenDocument.cmd("op", "setRunFormat", "paragraph", paragraph.getId(), "start", start, "end", start + length, "format", format));
    }

    private Object get(String key) {
        if (pending.containsKey(key)) {
            return pending.get(key);
        }
        for (OpenRun run : paragraph.getRuns()) {
            if (run.start <= start && start < run.start + Math.max(run.length, 1)) {
                return run.snapshot == null ? null : run.snapshot.get(key);
            }
        }
        return snapshot == null ? null : snapshot.get(key);
    }

    public void setBold(boolean value) {
        set("bold", value);
    }

    public boolean isBold() {
        return Boolean.TRUE.equals(get("bold"));
    }

    public void setItalic(boolean value) {
        set("italic", value);
    }

    public boolean isItalic() {
        return Boolean.TRUE.equals(get("italic"));
    }

    public void setUnderline(boolean value) {
        set("underline", value);
    }

    public void setStrike(boolean value) {
        set("strike", value);
    }

    /** Font size in points ({@code setFontSize}). */
    public void setFontSize(double points) {
        set("sizePt", points);
    }

    /** Font size in points, or -1 when inherited. */
    public double getFontSizeAsDouble() {
        Object size = get("sizePt");
        return size == null ? -1 : ((Number) size).doubleValue();
    }

    public void setFontFamily(String family) {
        set("font", family);
    }

    public String getFontFamily() {
        return (String) get("font");
    }

    /** Colour as {@code RRGGBB} ({@code setColor}). */
    public void setColor(String rgb) {
        set("color", rgb);
    }

    public String getColor() {
        return (String) get("color");
    }

    /** The hyperlink target the run sits in, or null. */
    public String getHyperlink() {
        return (String) get("hyperlink");
    }

    @Override
    public String toString() {
        return getText();
    }
}
