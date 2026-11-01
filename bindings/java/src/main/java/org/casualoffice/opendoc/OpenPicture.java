// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.io.IOException;
import java.io.InputStream;
import java.util.Base64;
import java.util.Map;

/**
 * A picture, with POI's {@code XWPFPicture} naming ({@code getWidth},
 * {@code getDepth}, {@code getDescription}). Resizing, alt text, replacing and
 * removing are additions: each is one engine edit.
 */
public final class OpenPicture {
    private final OpenDocument document;
    private final String id;

    OpenPicture(OpenDocument document, String id) {
        this.document = document;
        this.id = id;
    }

    static String encode(InputStream data) throws IOException {
        return Base64.getEncoder().encodeToString(data.readAllBytes());
    }

    /** The stable node id. */
    public String getId() {
        return id;
    }

    @SuppressWarnings("unchecked")
    private Map<String, Object> image() {
        for (Map<String, Object> run : document.allRuns()) {
            if (id.equals(run.get("id")) && run.get("image") instanceof Map<?, ?>) {
                return (Map<String, Object>) run.get("image");
            }
        }
        throw new OpenDocException("invalid-argument", "picture " + id + " is no longer in the document");
    }

    private double number(String key) {
        Object value = image().get(key);
        return value == null ? 0 : ((Number) value).doubleValue();
    }

    /** Width in points ({@code getWidth}). */
    public double getWidth() {
        return number("widthPt");
    }

    /** Height in points ({@code getDepth}). */
    public double getDepth() {
        return number("heightPt");
    }

    /** The alternative text ({@code getDescription}). */
    public String getDescription() {
        return (String) image().get("description");
    }

    /** Resizes the picture, in points. */
    public void setSize(double widthPt, double heightPt) {
        document.edit(OpenDocument.cmd("op", "resizePicture", "picture", id, "widthPt", widthPt, "heightPt", heightPt));
    }

    /** Sets the alternative text; {@code null} or empty clears it. */
    public void setDescription(String text) {
        document.edit(OpenDocument.cmd("op", "setAltText", "picture", id, "text", text == null || text.isEmpty() ? null : text));
    }

    /** Replaces the image, keeping the picture's frame, position and alt text. */
    public void replace(InputStream data) throws IOException {
        document.edit(OpenDocument.cmd("op", "replacePicture", "picture", id, "image", encode(data)));
    }

    /** Removes the picture. */
    public void remove() {
        document.edit(OpenDocument.cmd("op", "deletePicture", "picture", id));
    }
}
