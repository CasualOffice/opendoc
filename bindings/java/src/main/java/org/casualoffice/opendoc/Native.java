// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * The C ABI ({@code crates/opendoc-ffi/include/opendoc.h}) through the Foreign
 * Function and Memory API (JDK 22+, JEP 454): no JNI glue, no generated code.
 * Every handle and buffer the library returns is freed here exactly once.
 *
 * <p>The library is found through the system property {@code opendoc.library}
 * (a file path), the environment variable {@code OPENDOC_LIBRARY}, or
 * {@code java.library.path}.
 */
final class Native {
    static final int ABI_VERSION = 1;

    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup LIB = load();
    private static final ValueLayout.OfLong SIZE_T = ValueLayout.JAVA_LONG;

    private static final MethodHandle ABI = fn("opendoc_abi_version", FunctionDescriptor.of(ValueLayout.JAVA_INT));
    private static final MethodHandle VERSION = fn("opendoc_version", FunctionDescriptor.of(ValueLayout.ADDRESS));
    private static final MethodHandle LAST_ERROR = fn("opendoc_last_error", FunctionDescriptor.of(ValueLayout.ADDRESS));
    private static final MethodHandle OPEN = fn("opendoc_open",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, SIZE_T, ValueLayout.ADDRESS));
    private static final MethodHandle CREATE = fn("opendoc_create", FunctionDescriptor.of(ValueLayout.ADDRESS));
    private static final MethodHandle FROM_SPEC = fn("opendoc_from_spec",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS));
    private static final MethodHandle FREE = fn("opendoc_free", FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
    private static final MethodHandle CALL = fn("opendoc_call",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
    private static final MethodHandle SAVE = fn("opendoc_save",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
    private static final MethodHandle RENDER_PNG = fn("opendoc_render_png",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_INT, ValueLayout.JAVA_FLOAT));
    private static final MethodHandle RENDER_SVG = fn("opendoc_render_svg",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_INT));
    private static final MethodHandle TO_PDF = fn("opendoc_to_pdf",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS));
    private static final MethodHandle COMPARE = fn("opendoc_compare",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
    private static final MethodHandle CONVERT = fn("opendoc_convert",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, SIZE_T, ValueLayout.ADDRESS));
    private static final MethodHandle BUF_DATA = fn("opendoc_buffer_data",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS));
    private static final MethodHandle BUF_LEN = fn("opendoc_buffer_len",
            FunctionDescriptor.of(SIZE_T, ValueLayout.ADDRESS));
    private static final MethodHandle BUF_FREE = fn("opendoc_buffer_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    static {
        int abi;
        try {
            abi = (int) ABI.invokeExact();
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
        if (abi != ABI_VERSION) {
            throw new IllegalStateException("the opendoc library implements C ABI " + abi
                    + "; this jar was written for " + ABI_VERSION);
        }
    }

    private Native() {}

    private static SymbolLookup load() {
        String explicit = System.getProperty("opendoc.library", System.getenv("OPENDOC_LIBRARY"));
        if (explicit != null) {
            return SymbolLookup.libraryLookup(Path.of(explicit), Arena.global());
        }
        String file = System.mapLibraryName("opendoc_ffi");
        for (String dir : System.getProperty("java.library.path", "").split(java.io.File.pathSeparator)) {
            Path candidate = Path.of(dir, file);
            if (Files.isRegularFile(candidate)) {
                return SymbolLookup.libraryLookup(candidate, Arena.global());
            }
        }
        throw new IllegalStateException("cannot find " + file
                + "; set -Dopendoc.library=/path/to/" + file + " or OPENDOC_LIBRARY");
    }

    private static MethodHandle fn(String name, FunctionDescriptor descriptor) {
        MemorySegment symbol = LIB.find(name)
                .orElseThrow(() -> new IllegalStateException("the opendoc library lacks " + name));
        return LINKER.downcallHandle(symbol, descriptor);
    }

    /** Takes a returned buffer's bytes and frees it; a NULL buffer raises the last error. */
    static byte[] take(MemorySegment buffer) {
        if (buffer.equals(MemorySegment.NULL)) {
            throw lastError();
        }
        try {
            long length = (long) BUF_LEN.invokeExact(buffer);
            MemorySegment data = ((MemorySegment) BUF_DATA.invokeExact(buffer)).reinterpret(length);
            return data.toArray(ValueLayout.JAVA_BYTE);
        } catch (Throwable t) {
            throw rethrow(t);
        } finally {
            try {
                BUF_FREE.invokeExact(buffer);
            } catch (Throwable ignored) {
                // A free that throws leaves nothing to recover.
            }
        }
    }

    static String takeString(MemorySegment buffer) {
        return new String(take(buffer), StandardCharsets.UTF_8);
    }

    static OpenDocException lastError() {
        try {
            MemorySegment buffer = (MemorySegment) LAST_ERROR.invokeExact();
            if (buffer.equals(MemorySegment.NULL)) {
                return new OpenDocException("engine", "the call failed without a message");
            }
            return OpenDocException.of(new String(take(buffer), StandardCharsets.UTF_8));
        } catch (OpenDocException e) {
            return e;
        } catch (Throwable t) {
            return new OpenDocException("engine", String.valueOf(t));
        }
    }

    static RuntimeException rethrow(Throwable t) {
        if (t instanceof RuntimeException r) {
            return r;
        }
        return new IllegalStateException(t);
    }

    static MemorySegment handle(MemorySegment pointer) {
        if (pointer.equals(MemorySegment.NULL)) {
            throw lastError();
        }
        return pointer;
    }

    static String version() {
        try {
            return takeString((MemorySegment) VERSION.invokeExact());
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static MemorySegment open(byte[] data, String optionsJson) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment bytes = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            MemorySegment options = optionsJson == null ? MemorySegment.NULL : arena.allocateFrom(optionsJson);
            return handle((MemorySegment) OPEN.invokeExact(bytes, (long) data.length, options));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static MemorySegment create() {
        try {
            return handle((MemorySegment) CREATE.invokeExact());
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static MemorySegment fromSpec(String specJson) {
        try (Arena arena = Arena.ofConfined()) {
            return handle((MemorySegment) FROM_SPEC.invokeExact(arena.allocateFrom(specJson)));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static void free(MemorySegment doc) {
        try {
            FREE.invokeExact(doc);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static Object call(MemorySegment doc, String method, Object arg) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment argument = arg == null ? MemorySegment.NULL : arena.allocateFrom(Json.write(arg));
            String result = takeString((MemorySegment) CALL.invokeExact(doc, arena.allocateFrom(method), argument));
            return Json.parse(result);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static byte[] save(MemorySegment doc, String format, String mode) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment m = mode == null ? MemorySegment.NULL : arena.allocateFrom(mode);
            return take((MemorySegment) SAVE.invokeExact(doc, arena.allocateFrom(format), m));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static byte[] renderPng(MemorySegment doc, int page, float dpi) {
        try {
            return take((MemorySegment) RENDER_PNG.invokeExact(doc, page, dpi));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static byte[] renderSvg(MemorySegment doc, int page) {
        try {
            return take((MemorySegment) RENDER_SVG.invokeExact(doc, page));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static byte[] toPdf(MemorySegment doc) {
        try {
            return take((MemorySegment) TO_PDF.invokeExact(doc));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static String compare(MemorySegment older, MemorySegment newer) {
        try {
            return takeString((MemorySegment) COMPARE.invokeExact(older, newer));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static byte[] convert(byte[] data, String to) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment bytes = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            return take((MemorySegment) CONVERT.invokeExact(bytes, (long) data.length, arena.allocateFrom(to)));
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }
}
