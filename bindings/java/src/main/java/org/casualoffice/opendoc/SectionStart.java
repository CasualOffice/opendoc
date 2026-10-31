// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

/** How a section starts: OOXML's {@code ST_SectionMark} ({@code STSectionMark} in POI's schema classes). */
public enum SectionStart {
    NEXT_PAGE("nextPage"),
    CONTINUOUS("continuous"),
    EVEN_PAGE("evenPage"),
    ODD_PAGE("oddPage"),
    NEXT_COLUMN("nextColumn");

    private final String value;

    SectionStart(String value) {
        this.value = value;
    }

    String value() {
        return value;
    }

    static SectionStart of(String value) {
        for (SectionStart start : values()) {
            if (start.value.equals(value)) {
                return start;
            }
        }
        return NEXT_PAGE;
    }
}
