// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The minimal JSON the binding needs to read the engine's views and write edit
 * commands, so the jar has no dependency. Objects are {@code Map<String, Object>}
 * (insertion-ordered), arrays {@code List<Object>}, numbers {@code Long} or
 * {@code Double}, plus {@code String}, {@code Boolean} and {@code null}.
 */
public final class Json {
    private final String text;
    private int at;

    private Json(String text) {
        this.text = text;
    }

    /** Parses one JSON value. */
    public static Object parse(String text) {
        Json parser = new Json(text);
        parser.space();
        Object value = parser.value();
        parser.space();
        if (parser.at != text.length()) {
            throw new IllegalArgumentException("trailing characters in JSON at " + parser.at);
        }
        return value;
    }

    /** Serialises maps, lists, strings, numbers, booleans and null. */
    public static String write(Object value) {
        StringBuilder out = new StringBuilder();
        write(value, out);
        return out.toString();
    }

    @SuppressWarnings("unchecked")
    private static void write(Object value, StringBuilder out) {
        if (value == null) {
            out.append("null");
        } else if (value instanceof String s) {
            out.append('"');
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                switch (c) {
                    case '"' -> out.append("\\\"");
                    case '\\' -> out.append("\\\\");
                    case '\n' -> out.append("\\n");
                    case '\r' -> out.append("\\r");
                    case '\t' -> out.append("\\t");
                    default -> {
                        if (c < 0x20) {
                            out.append(String.format("\\u%04x", (int) c));
                        } else {
                            out.append(c);
                        }
                    }
                }
            }
            out.append('"');
        } else if (value instanceof Number || value instanceof Boolean) {
            out.append(value);
        } else if (value instanceof Map<?, ?> map) {
            out.append('{');
            boolean first = true;
            for (Map.Entry<?, ?> entry : map.entrySet()) {
                if (!first) {
                    out.append(',');
                }
                first = false;
                write(String.valueOf(entry.getKey()), out);
                out.append(':');
                write(entry.getValue(), out);
            }
            out.append('}');
        } else if (value instanceof Iterable<?> list) {
            out.append('[');
            boolean first = true;
            for (Object item : list) {
                if (!first) {
                    out.append(',');
                }
                first = false;
                write(item, out);
            }
            out.append(']');
        } else {
            throw new IllegalArgumentException("not JSON-serialisable: " + value.getClass());
        }
    }

    private void space() {
        while (at < text.length() && Character.isWhitespace(text.charAt(at))) {
            at++;
        }
    }

    private Object value() {
        if (at >= text.length()) {
            throw new IllegalArgumentException("unexpected end of JSON");
        }
        char c = text.charAt(at);
        switch (c) {
            case '{':
                return object();
            case '[':
                return array();
            case '"':
                return string();
            case 't':
                expect("true");
                return Boolean.TRUE;
            case 'f':
                expect("false");
                return Boolean.FALSE;
            case 'n':
                expect("null");
                return null;
            default:
                return number();
        }
    }

    private void expect(String word) {
        if (!text.startsWith(word, at)) {
            throw new IllegalArgumentException("bad JSON at " + at);
        }
        at += word.length();
    }

    private Map<String, Object> object() {
        Map<String, Object> map = new LinkedHashMap<>();
        at++;
        space();
        if (text.charAt(at) == '}') {
            at++;
            return map;
        }
        while (true) {
            space();
            String key = string();
            space();
            if (text.charAt(at++) != ':') {
                throw new IllegalArgumentException("expected ':' at " + (at - 1));
            }
            space();
            map.put(key, value());
            space();
            char next = text.charAt(at++);
            if (next == '}') {
                return map;
            }
            if (next != ',') {
                throw new IllegalArgumentException("expected ',' at " + (at - 1));
            }
        }
    }

    private List<Object> array() {
        List<Object> list = new ArrayList<>();
        at++;
        space();
        if (text.charAt(at) == ']') {
            at++;
            return list;
        }
        while (true) {
            space();
            list.add(value());
            space();
            char next = text.charAt(at++);
            if (next == ']') {
                return list;
            }
            if (next != ',') {
                throw new IllegalArgumentException("expected ',' at " + (at - 1));
            }
        }
    }

    private String string() {
        if (text.charAt(at) != '"') {
            throw new IllegalArgumentException("expected a string at " + at);
        }
        at++;
        StringBuilder out = new StringBuilder();
        while (true) {
            char c = text.charAt(at++);
            if (c == '"') {
                return out.toString();
            }
            if (c != '\\') {
                out.append(c);
                continue;
            }
            char e = text.charAt(at++);
            switch (e) {
                case 'n' -> out.append('\n');
                case 't' -> out.append('\t');
                case 'r' -> out.append('\r');
                case 'b' -> out.append('\b');
                case 'f' -> out.append('\f');
                case 'u' -> {
                    out.append((char) Integer.parseInt(text.substring(at, at + 4), 16));
                    at += 4;
                }
                default -> out.append(e);
            }
        }
    }

    private Number number() {
        int start = at;
        while (at < text.length() && "+-0123456789.eE".indexOf(text.charAt(at)) >= 0) {
            at++;
        }
        String raw = text.substring(start, at);
        if (raw.isEmpty()) {
            throw new IllegalArgumentException("bad JSON at " + start);
        }
        if (raw.contains(".") || raw.contains("e") || raw.contains("E")) {
            return Double.parseDouble(raw);
        }
        return Long.parseLong(raw);
    }
}
