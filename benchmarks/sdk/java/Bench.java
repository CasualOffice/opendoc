// SPDX-License-Identifier: Apache-2.0
//
// One Java measurement for benchmarks/sdk/run.mjs: the same POI-style program
// against Apache POI XWPF and against the OpenDoc Java binding (whose naming is
// POI's). Prints one JSON line: median milliseconds and an output size.
//
//   java -cp <poi jars>:<opendoc classes>:. Bench <generate|read> <poi|opendoc> <n|path> <iterations>
import java.io.ByteArrayOutputStream;
import java.io.FileInputStream;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import org.apache.poi.xwpf.usermodel.XWPFDocument;
import org.apache.poi.xwpf.usermodel.XWPFParagraph;
import org.apache.poi.xwpf.usermodel.XWPFRun;
import org.apache.poi.xwpf.usermodel.XWPFTable;
import org.casualoffice.opendoc.OpenDocument;
import org.casualoffice.opendoc.OpenParagraph;
import org.casualoffice.opendoc.OpenRun;
import org.casualoffice.opendoc.OpenTable;

public final class Bench {
    private Bench() {}

    static long poiGenerate(int n) throws Exception {
        try (XWPFDocument doc = new XWPFDocument()) {
            XWPFRun heading = doc.createParagraph().createRun();
            heading.setText("Benchmark report");
            heading.setBold(true);
            for (int i = 0; i < n; i++) {
                XWPFParagraph p = doc.createParagraph();
                p.createRun().setText("Paragraph " + i + ": the quick brown fox jumps over the lazy dog. ");
                XWPFRun bold = p.createRun();
                bold.setBold(true);
                bold.setText("Bold");
            }
            int rows = Math.max(10, n / 10);
            XWPFTable table = doc.createTable(rows, 3);
            for (int r = 0; r < rows; r++) {
                table.getRow(r).getCell(0).setText("r" + r + "c0");
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            doc.write(out);
            return out.size();
        }
    }

    static long openDocGenerate(int n) throws Exception {
        try (OpenDocument doc = new OpenDocument()) {
            OpenRun heading = doc.createParagraph().createRun();
            heading.setText("Benchmark report");
            heading.setBold(true);
            for (int i = 0; i < n; i++) {
                OpenParagraph p = doc.createParagraph();
                p.createRun().setText("Paragraph " + i + ": the quick brown fox jumps over the lazy dog. ");
                OpenRun bold = p.createRun();
                bold.setBold(true);
                bold.setText("Bold");
            }
            int rows = Math.max(10, n / 10);
            OpenTable table = doc.createTable(rows, 3);
            for (int r = 0; r < rows; r++) {
                table.getRow(r).getCell(0).setText("r" + r + "c0");
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            doc.write(out);
            return out.size();
        }
    }

    static long poiRead(String path) throws Exception {
        try (InputStream in = new FileInputStream(path); XWPFDocument doc = new XWPFDocument(in)) {
            long total = 0;
            for (XWPFParagraph p : doc.getParagraphs()) {
                total += p.getText().length();
            }
            return total;
        }
    }

    static long openDocRead(String path) throws Exception {
        try (InputStream in = new FileInputStream(path); OpenDocument doc = new OpenDocument(in)) {
            long total = 0;
            for (OpenParagraph p : doc.getParagraphs()) {
                total += p.getText().length();
            }
            return total;
        }
    }

    static long run(String task, String library, String input) throws Exception {
        boolean poi = library.equals("poi");
        if (task.equals("generate")) {
            int n = Integer.parseInt(input);
            return poi ? poiGenerate(n) : openDocGenerate(n);
        }
        return poi ? poiRead(input) : openDocRead(input);
    }

    public static void main(String[] args) throws Exception {
        String task = args[0];
        String library = args[1];
        String input = args[2];
        int iterations = Integer.parseInt(args[3]);
        long output = run(task, library, input); // warm-up (JIT, class loading)
        List<Double> times = new ArrayList<>();
        for (int i = 0; i < iterations; i++) {
            long start = System.nanoTime();
            output = run(task, library, input);
            times.add((System.nanoTime() - start) / 1e6);
        }
        Collections.sort(times);
        double median = times.get(times.size() / 2);
        System.out.printf("{\"ms\": %.2f, \"output\": %d}%n", median, output);
    }
}
