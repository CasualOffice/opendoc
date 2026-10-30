// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

/** Paragraph alignment, POI's {@code ParagraphAlignment} names. */
public enum ParagraphAlignment {
    LEFT("left"),
    CENTER("center"),
    RIGHT("right"),
    BOTH("justify");

    private final String value;

    ParagraphAlignment(String value) {
        this.value = value;
    }

    String value() {
        return value;
    }
}
