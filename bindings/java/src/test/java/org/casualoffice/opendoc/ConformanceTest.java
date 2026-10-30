// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

/**
 * The Java binding, driven as a POI user drives XWPF. A plain main with no test
 * framework, so the jar has no dependency: it exits non-zero on the first
 * failure and prints each check as it passes.
 *
 * <pre>java --enable-native-access=ALL-UNNAMED -Dopendoc.library=... -cp build/classes:build/test-classes \
 *     org.casualoffice.opendoc.ConformanceTest path/to/sample.docx</pre>
 */
public final class ConformanceTest {
    private static int passed;

    private ConformanceTest() {}

    private static void check(boolean condition, String what) {
        if (!condition) {
            throw new AssertionError("FAILED: " + what);
        }
        passed++;
        System.out.println("ok " + passed + " - " + what);
    }

    public static void main(String[] args) throws Exception {
        Path sample = Path.of(args.length > 0 ? args[0] : "sample.docx");
        byte[] sampleBytes = Files.readAllBytes(sample);

        // POI's create-a-document flow, formatting set before and after text.
        byte[] built;
        try (OpenDocument doc = new OpenDocument()) {
            OpenParagraph title = doc.createParagraph();
            title.setStyle("Title");
            title.createRun().setText("Quarterly report");
            OpenParagraph p = doc.createParagraph();
            OpenRun plain = p.createRun();
            plain.setText("Revenue grew ");
            OpenRun bold = p.createRun();
            bold.setBold(true);
            bold.setColor("2E7D32");
            bold.setText("12%");
            OpenRun tail = p.createRun();
            tail.setText(" this quarter.");
            tail.setItalic(true);
            p.setAlignment(ParagraphAlignment.CENTER);
            OpenTable table = doc.createTable(2, 3);
            table.getRow(0).getCell(0).setText("Region");
            table.getRow(1).getCell(2).setText("1,350");
            table.createRow().getCell(1).setText("added");
            doc.setProperties(Map.of("title", "Report", "creator", "Finance"));
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            doc.write(out);
            built = out.toByteArray();
            check(((List<?>) doc.getLastSaveReport().get("losses")).isEmpty(), "a built document saves with no losses");
        }
        try (OpenDocument doc = new OpenDocument(new ByteArrayInputStream(built))) {
            List<OpenParagraph> ps = doc.getParagraphs();
            OpenParagraph p = ps.stream().filter(x -> x.getText().startsWith("Revenue")).findFirst().orElseThrow();
            check(p.getText().equals("Revenue grew 12% this quarter."), "runs concatenate in order");
            List<OpenRun> runs = p.getRuns();
            OpenRun twelve = runs.stream().filter(r -> r.getText().equals("12%")).findFirst().orElseThrow();
            check(twelve.isBold() && "2E7D32".equals(twelve.getColor()), "format set before the text lands on it");
            OpenRun tail = runs.stream().filter(r -> r.getText().equals(" this quarter.")).findFirst().orElseThrow();
            check(tail.isItalic() && !tail.isBold(), "format set after the text lands on it alone");
            check(ps.stream().anyMatch(x -> x.getHeadingLevel() == 0), "the Title style reads back");
            OpenTable table = doc.getTables().get(0);
            check(table.getNumberOfRows() == 3, "createRow appends a row");
            check(table.getRow(1).getCell(2).getText().equals("1,350"), "cell text reads back");
            check("Report".equals(doc.getProperties().get("title")), "core properties read back");
        }

        // Open, read, edit, save, render.
        try (OpenDocument doc = new OpenDocument(sampleBytes)) {
            check("docx".equals(doc.getImportReport().get("format")), "the import report names the format");
            String before = doc.getText();
            int replaced = doc.replace("the", "THE");
            check(replaced > 0 && doc.getText().contains("THE"), "replace counts and changes text");
            check(doc.getPageCount() > 1, "the engine paginates");
            byte[] png = doc.renderPagePng(0, 24f);
            check(png.length > 8 && png[1] == 'P' && png[2] == 'N' && png[3] == 'G', "a page renders to PNG");
            byte[] pdf = doc.toPdf();
            check(new String(pdf, 0, 5, StandardCharsets.US_ASCII).equals("%PDF-"), "the document writes PDF");
            Path out = Files.createTempFile("opendoc-java", ".odt");
            Map<String, Object> report = doc.save(out);
            check("odt".equals(report.get("format")) && Files.size(out) > 0, "save picks the format from the extension");
            try {
                doc.toBytes("docx", "exact");
                check(false, "an exact save of an edited document is refused");
            } catch (OpenDocException e) {
                check("save".equals(e.getCode()), "an exact save of an edited document is refused with code save");
            }
            check(!before.equals(doc.getText()), "the edit is visible");
        }

        // Errors carry a stable code; a failed batch changes nothing.
        try {
            new OpenDocument("PK\u0003\u0004 junk".getBytes(StandardCharsets.ISO_8859_1)).close();
            check(false, "junk is refused");
        } catch (OpenDocException e) {
            check("open".equals(e.getCode()), "junk is refused with code open");
        }
        try (OpenDocument doc = new OpenDocument(sampleBytes)) {
            String before = doc.getText();
            try {
                doc.editCommands(
                        OpenDocument.cmd("op", "replaceAll", "find", "the", "replace", "THE"),
                        OpenDocument.cmd("op", "deleteBlock", "id", "nope"));
                check(false, "a bad batch is refused");
            } catch (OpenDocException e) {
                check("invalid-argument".equals(e.getCode()) && doc.getText().equals(before), "a refused batch changes nothing");
            }
        }

        // Templates.
        String spec = "{\"sections\":[{\"children\":["
                + "{\"type\":\"paragraph\",\"text\":\"Dear {name},\"},"
                + "{\"type\":\"paragraph\",\"text\":\"{#items}\"},"
                + "{\"type\":\"paragraph\",\"text\":\"- {.}\"},"
                + "{\"type\":\"paragraph\",\"text\":\"{/items}\"}]}]}";
        try (OpenDocument doc = OpenDocument.fromSpec(spec)) {
            doc.renderTemplate(Map.of("name", "Ada", "items", List.of("one", "two")));
            List<String> texts = doc.getParagraphs().stream().map(OpenParagraph::getText).toList();
            check(texts.equals(List.of("Dear Ada,", "- one", "- two")), "a template fills values and loops");
        }
        byte[] text = OpenDocument.convert(sampleBytes, "txt");
        check(text.length > 0, "convert writes text");
        check(!OpenDocument.version().isEmpty(), "the engine reports its version");
        System.out.println("all " + passed + " checks passed");
    }
}
