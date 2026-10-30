// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

/**
 * Every error the SDK raises. {@link #getCode()} is stable: {@code open},
 * {@code unsupported-format}, {@code save}, {@code edit},
 * {@code invalid-argument}, {@code build}, {@code template}, {@code render},
 * {@code exhausted}, {@code engine}.
 */
public class OpenDocException extends RuntimeException {
    private static final long serialVersionUID = 1L;
    private final String code;

    public OpenDocException(String code, String message) {
        super(code + ": " + message);
        this.code = code;
    }

    static OpenDocException of(String coded) {
        int colon = coded.indexOf(": ");
        if (colon > 0 && coded.substring(0, colon).matches("[a-z-]+")) {
            return new OpenDocException(coded.substring(0, colon), coded.substring(colon + 2));
        }
        return new OpenDocException("engine", coded);
    }

    /** The stable error code. */
    public String getCode() {
        return code;
    }
}
