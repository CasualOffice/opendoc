// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.foreign.MemorySegment;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A word-processing document, with Apache POI XWPF's naming ({@code XWPFDocument}
 * becomes {@code OpenDocument}):
 *
 * <pre>{@code
 * try (OpenDocument doc = new OpenDocument()) {
 *     OpenParagraph p = doc.createParagraph();
 *     OpenRun r = p.createRun();
 *     r.setBold(true);
 *     r.setText("Hello");
 *     OpenTable t = doc.createTable(2, 3);
 *     t.getRow(0).getCell(0).setText("Qty");
 *     try (OutputStream out = Files.newOutputStream(Path.of("out.docx"))) {
 *         doc.write(out);
 *     }
 *     doc.save(Path.of("out.pdf"));        // Aspose-style: the format from the extension
 * }
 * }</pre>
 *
 * <p>What POI does not have: {@link #save(Path)} to PDF, ODT, HTML, Markdown or
 * text from the engine's own layout; {@link #getPageCount()};
 * {@link #renderPagePng(int, float)}; templates ({@link #renderTemplate(Map)});
 * a loss report on open ({@link #getImportReport()}) and on every save
 * ({@link #getLastSaveReport()}). Every edit is an engine operation, applied
 * all or nothing; POI's own documentation warns its low-level API "may" write
 * invalid files.
 *
 * <p>An instance owns native memory: close it ({@code try}-with-resources).
 * It is not thread-safe.
 */
public final class OpenDocument implements AutoCloseable {
    private MemorySegment handle;
    private final Map<String, OpenParagraph> paragraphs = new LinkedHashMap<>();

    private OpenDocument(MemorySegment handle) {
        this.handle = handle;
    }

    /** A blank document with Word's built-in styles (POI's {@code new XWPFDocument()}). */
    public OpenDocument() {
        this(Native.create());
    }

    /** Opens DOCX, ODT, RTF, TXT or JSON from a stream (POI's {@code new XWPFDocument(InputStream)}). */
    public OpenDocument(InputStream in) throws IOException {
        this(Native.open(in.readAllBytes(), null));
    }

    /** Opens bytes. */
    public OpenDocument(byte[] data) {
        this(Native.open(data, null));
    }

    /** Opens a file. */
    public static OpenDocument open(Path path) throws IOException {
        Map<String, Object> options = new LinkedHashMap<>();
        options.put("fileName", path.getFileName().toString());
        return new OpenDocument(Native.open(Files.readAllBytes(path), Json.write(options)));
    }

    /** Builds a document from a docx-js-shaped spec (JSON; images as base64 {@code data}). */
    public static OpenDocument fromSpec(String specJson) {
        return new OpenDocument(Native.fromSpec(specJson));
    }

    /** Converts bytes to another format in one call. */
    public static byte[] convert(byte[] data, String toFormat) {
        return Native.convert(data, toFormat);
    }

    /** The engine version. */
    public static String version() {
        return Native.version();
    }

    MemorySegment handle() {
        if (handle == null) {
            throw new OpenDocException("invalid-argument", "this document was closed");
        }
        return handle;
    }

    Object call(String method, Object arg) {
        return Native.call(handle(), method, arg);
    }

    @SafeVarargs
    @SuppressWarnings("unchecked")
    final List<Object> edit(Map<String, Object>... commands) {
        List<Object> list = new ArrayList<>();
        for (Map<String, Object> command : commands) {
            Map<String, Object> clean = new LinkedHashMap<>();
            command.forEach((k, v) -> {
                if (v != null) {
                    clean.put(k, v);
                }
            });
            list.add(clean);
        }
        Map<String, Object> result = (Map<String, Object>) call("edit", list);
        return (List<Object>) result.get("results");
    }

    static Map<String, Object> cmd(Object... pairs) {
        Map<String, Object> map = new LinkedHashMap<>();
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            map.put((String) pairs[i], pairs[i + 1]);
        }
        return map;
    }

    @SuppressWarnings("unchecked")
    Map<String, Object> view(String id) {
        Object paragraph = call("paragraph", cmd("id", id));
        if (paragraph instanceof Map<?, ?>) {
            return (Map<String, Object>) paragraph;
        }
        Object table = call("table", cmd("id", id));
        if (table instanceof Map<?, ?>) {
            return (Map<String, Object>) table;
        }
        throw new OpenDocException("invalid-argument", "node " + id + " is no longer in the document");
    }

    /** A table's view, looked up as a table (no paragraph-locator miss first). */
    @SuppressWarnings("unchecked")
    Map<String, Object> tableView(String id) {
        Object table = call("table", cmd("id", id));
        if (table instanceof Map<?, ?>) {
            return (Map<String, Object>) table;
        }
        throw new OpenDocException("invalid-argument", "table " + id + " is no longer in the document");
    }

    OpenParagraph paragraph(String id) {
        return paragraphs.computeIfAbsent(id, key -> new OpenParagraph(this, key));
    }

    // --- read (POI) ---

    /** The body's top-level paragraphs ({@code getParagraphs}). */
    @SuppressWarnings("unchecked")
    public List<OpenParagraph> getParagraphs() {
        List<OpenParagraph> out = new ArrayList<>();
        for (Object block : (List<Object>) call("body", null)) {
            Map<String, Object> b = (Map<String, Object>) block;
            if ("paragraph".equals(b.get("type"))) {
                out.add(paragraph((String) b.get("id")));
            }
        }
        return out;
    }

    /** The body's top-level tables ({@code getTables}). */
    @SuppressWarnings("unchecked")
    public List<OpenTable> getTables() {
        List<OpenTable> out = new ArrayList<>();
        for (Object block : (List<Object>) call("body", null)) {
            Map<String, Object> b = (Map<String, Object>) block;
            if ("table".equals(b.get("type"))) {
                out.add(new OpenTable(this, (String) b.get("id")));
            }
        }
        return out;
    }

    /** The body's plain text (POI's {@code XWPFWordExtractor.getText()}). */
    public String getText() {
        return (String) call("text", null);
    }

    /** Comments, with author, text and anchor. */
    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> getComments() {
        return (List<Map<String, Object>>) call("comments", null);
    }

    /** Tracked changes. */
    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> getRevisions() {
        return (List<Map<String, Object>>) call("revisions", null);
    }

    /** Section page setup. */
    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> getSections() {
        return (List<Map<String, Object>>) call("sections", null);
    }

    /** Headers and footers. */
    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> getHeadersFooters() {
        return (List<Map<String, Object>>) call("headersFooters", null);
    }

    /** Core and custom properties. */
    @SuppressWarnings("unchecked")
    public Map<String, Object> getProperties() {
        return (Map<String, Object>) call("properties", null);
    }

    /** What the open did not carry into the model ({@code findings}) and what it repaired. */
    @SuppressWarnings("unchecked")
    public Map<String, Object> getImportReport() {
        return (Map<String, Object>) call("importReport", null);
    }

    // --- modify (POI) ---

    /** Appends a paragraph ({@code createParagraph}). */
    public OpenParagraph createParagraph() {
        return paragraph((String) edit(cmd("op", "insertParagraph", "text", "")).get(0));
    }

    /** Appends a {@code rows} x {@code cols} table ({@code createTable(rows, cols)}). */
    public OpenTable createTable(int rows, int cols) {
        return new OpenTable(this, (String) edit(cmd("op", "insertTable", "rows", rows, "columns", cols)).get(0));
    }

    /** Sets core properties: {@code title}, {@code subject}, {@code creator}, {@code keywords}, {@code description}, {@code lastModifiedBy}, {@code category}. */
    public void setProperties(Map<String, String> properties) {
        edit(cmd("op", "setProperties", "properties", new LinkedHashMap<>(properties)));
    }

    /** Replaces every occurrence in every story; returns the count (Aspose's {@code Range.replace}). */
    public int replace(String find, String replacement) {
        return ((Number) edit(cmd("op", "replaceAll", "find", find, "replace", replacement)).get(0)).intValue();
    }

    /** Accepts every tracked change (Aspose's {@code acceptAllRevisions}). */
    public boolean acceptAllRevisions() {
        return Boolean.TRUE.equals(edit(cmd("op", "acceptAllRevisions")).get(0));
    }

    /** Rejects every tracked change. */
    public boolean rejectAllRevisions() {
        return Boolean.TRUE.equals(edit(cmd("op", "rejectAllRevisions")).get(0));
    }

    // --- headers, footers, notes, sections, pictures (docs/176 section 12.12) ---

    /**
     * The first section's header of {@code type} ({@code createHeader(HeaderFooterType)}).
     * As in POI, a {@code FIRST} header turns the section's different first page on
     * and an {@code EVEN} one turns different odd and even pages on.
     */
    public OpenHeaderFooter createHeader(HeaderFooterType type) {
        return createHeader(0, type);
    }

    /** Section {@code section}'s header of {@code type}. */
    public OpenHeaderFooter createHeader(int section, HeaderFooterType type) {
        return band(section, "header", type);
    }

    /** The first section's footer of {@code type} ({@code createFooter(HeaderFooterType)}). */
    public OpenHeaderFooter createFooter(HeaderFooterType type) {
        return createFooter(0, type);
    }

    /** Section {@code section}'s footer of {@code type}. */
    public OpenHeaderFooter createFooter(int section, HeaderFooterType type) {
        return band(section, "footer", type);
    }

    private OpenHeaderFooter band(int section, String region, HeaderFooterType type) {
        List<Map<String, Object>> commands = new ArrayList<>();
        if (type == HeaderFooterType.FIRST) {
            commands.add(cmd("op", "setDifferentFirstPage", "section", section, "on", true));
        } else if (type == HeaderFooterType.EVEN) {
            commands.add(cmd("op", "setDifferentOddEven", "on", true));
        }
        commands.add(cmd("op", "headerFooter", "section", section, "region", region, "kind", type.value()));
        editList(commands);
        return new OpenHeaderFooter(this, section, region, type);
    }

    /** Footnotes and endnotes, with their text. */
    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> getNotes() {
        return (List<Map<String, Object>>) call("notes", null);
    }

    /** The footnotes ({@code getFootnotes}). */
    public List<OpenNote> getFootnotes() {
        return notesOf("footnote");
    }

    /** The endnotes. */
    public List<OpenNote> getEndnotes() {
        return notesOf("endnote");
    }

    private List<OpenNote> notesOf(String kind) {
        List<OpenNote> out = new ArrayList<>();
        for (Map<String, Object> note : getNotes()) {
            if (kind.equals(note.get("kind"))) {
                out.add(new OpenNote(this, (String) note.get("id")));
            }
        }
        return out;
    }

    /** The sections, by index ({@code getSections} returns their raw views). */
    public List<OpenSection> getSectionList() {
        List<OpenSection> out = new ArrayList<>();
        for (int i = 0; i < getSections().size(); i++) {
            out.add(new OpenSection(this, i));
        }
        return out;
    }

    /** Adds a section at the end, inheriting the last one's page setup; returns it. */
    public OpenSection addSection(SectionStart start) {
        int index = ((Number) edit(cmd("op", "addSection", "start", start.value())).get(0)).intValue();
        return new OpenSection(this, index);
    }

    /** Different odd and even page headers and footers, document-wide. */
    public void setEvenAndOddHeaders(boolean on) {
        edit(cmd("op", "setDifferentOddEven", "on", on));
    }

    /** Every picture in the body, headers, footers and notes, in order. */
    public List<OpenPicture> getAllPictures() {
        List<OpenPicture> out = new ArrayList<>();
        for (Map<String, Object> run : allRuns()) {
            if ("image".equals(run.get("kind"))) {
                out.add(new OpenPicture(this, (String) run.get("id")));
            }
        }
        return out;
    }

    /** Every run of every paragraph, body first, then headers and footers, then notes. */
    @SuppressWarnings("unchecked")
    List<Map<String, Object>> allRuns() {
        List<Map<String, Object>> out = new ArrayList<>();
        collect((List<Object>) call("body", null), out);
        for (Map<String, Object> story : getHeadersFooters()) {
            collect((List<Object>) story.get("blocks"), out);
        }
        for (Map<String, Object> note : getNotes()) {
            collect((List<Object>) note.get("blocks"), out);
        }
        return out;
    }

    @SuppressWarnings("unchecked")
    private static void collect(List<Object> blocks, List<Map<String, Object>> out) {
        for (Object raw : blocks) {
            Map<String, Object> block = (Map<String, Object>) raw;
            Object type = block.get("type");
            if ("paragraph".equals(type)) {
                for (Object run : (List<Object>) block.get("runs")) {
                    out.add((Map<String, Object>) run);
                }
            } else if ("table".equals(type)) {
                for (Object row : (List<Object>) block.get("rows")) {
                    for (Object cell : (List<Object>) ((Map<String, Object>) row).get("cells")) {
                        collect((List<Object>) ((Map<String, Object>) cell).get("blocks"), out);
                    }
                }
            } else if ("contentControl".equals(type)) {
                collect((List<Object>) block.get("blocks"), out);
            }
        }
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    final List<Object> editList(List<Map<String, Object>> commands) {
        return edit(commands.toArray(new Map[0]));
    }

    /** Runs edit commands (the JSON vocabulary every binding shares) as one atomic batch. */
    @SafeVarargs
    public final List<Object> editCommands(Map<String, Object>... commands) {
        return edit(commands);
    }

    /**
     * Fills docxtemplater-style tags ({@code {name}}, {@code {#items}...{/items}},
     * {@code {^flag}...{/flag}}, {@code {.}}) with {@code data}. Throws an
     * {@link OpenDocException} with code {@code template} listing every issue.
     */
    public void renderTemplate(Map<String, Object> data) {
        call("renderTemplate", cmd("data", data));
    }

    // --- save and render ---

    /** Writes DOCX to the stream ({@code write(OutputStream)}). */
    public void write(OutputStream out) throws IOException {
        out.write(toBytes("docx", null));
    }

    /** Saves; the format comes from the extension (Aspose's {@code save(String)}). Returns the save report. */
    public Map<String, Object> save(Path path) throws IOException {
        String name = path.getFileName().toString();
        int dot = name.lastIndexOf('.');
        String format = dot >= 0 ? name.substring(dot + 1).toLowerCase(java.util.Locale.ROOT) : "docx";
        Files.write(path, toBytes(format, null));
        return getLastSaveReport();
    }

    /** The bytes of a save to {@code format}; {@code mode} is null, "exact", "preserve" or "semantic". */
    public byte[] toBytes(String format, String mode) {
        return Native.save(handle(), format, mode);
    }

    /** The last save's report: {@code losses} empty means nothing was lost. */
    @SuppressWarnings("unchecked")
    public Map<String, Object> getLastSaveReport() {
        return (Map<String, Object>) call("lastSaveReport", null);
    }

    /** The number of laid-out pages (Aspose's {@code getPageCount}). */
    public int getPageCount() {
        return ((Number) call("pageCount", null)).intValue();
    }

    /** One page's text. */
    public String getPageText(int index) {
        return (String) call("pageText", cmd("index", index));
    }

    /** One page as PNG. */
    public byte[] renderPagePng(int page, float dpi) {
        return Native.renderPng(handle(), page, dpi);
    }

    /** One page as SVG, from the same layout as the PNG and the PDF (glyph outlines, a transparent real-text layer). */
    public String renderPageSvg(int page) {
        return new String(Native.renderSvg(handle(), page), java.nio.charset.StandardCharsets.UTF_8);
    }

    /** A real-text PDF from the same layout. */
    public byte[] toPdf() {
        return Native.toPdf(handle());
    }

    /** The structural diff of this document (older) against {@code newer}, as JSON. */
    public String compare(OpenDocument newer) {
        return Native.compare(handle(), newer.handle());
    }

    /** Releases the native memory. */
    @Override
    public void close() {
        if (handle != null) {
            Native.free(handle);
            handle = null;
        }
    }

}
