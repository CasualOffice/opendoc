// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

/** Which pages a header or footer applies to, POI's {@code HeaderFooterType} names. */
public enum HeaderFooterType {
    DEFAULT("default"),
    FIRST("first"),
    EVEN("even");

    private final String value;

    HeaderFooterType(String value) {
        this.value = value;
    }

    String value() {
        return value;
    }
}
