// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc.examples;

import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;

import org.casualoffice.opendoc.OpenDocument;
import org.casualoffice.opendoc.OpenParagraph;
import org.casualoffice.opendoc.OpenRun;
import org.casualoffice.opendoc.OpenTable;
import org.casualoffice.opendoc.ParagraphAlignment;

/** POI XWPF calls, then what POI cannot do: a PDF and a page image. build.sh runs it. */
public final class Quickstart {
    private Quickstart() {}

    public static void main(String[] args) throws Exception {
        Path out = Files.createDirectories(Path.of(args.length > 0 ? args[0] : "out"));
        try (OpenDocument doc = new OpenDocument()) {
            OpenParagraph title = doc.createParagraph();
            title.setAlignment(ParagraphAlignment.CENTER);
            OpenRun run = title.createRun();
            run.setBold(true);
            run.setText("Quarterly report");
            OpenTable table = doc.createTable(2, 2);
            table.getRow(0).getCell(0).setText("Region");
            table.getRow(1).getCell(0).setText("North");
            try (OutputStream stream = Files.newOutputStream(out.resolve("report.docx"))) {
                doc.write(stream);
            }
            doc.save(out.resolve("report.pdf"));
            Files.write(out.resolve("page-1.png"), doc.renderPagePng(0, 96f));
            System.out.println(doc.getPageCount() + " page(s); lost on save: "
                + doc.getLastSaveReport().get("losses"));
        }
    }
}
