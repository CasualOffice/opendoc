# OpenDoc for Java

The OpenDoc engine with Apache POI XWPF's naming. `XWPFDocument` becomes `OpenDocument`,
`XWPFParagraph` becomes `OpenParagraph`, `XWPFRun` becomes `OpenRun` and `XWPFTable` becomes
`OpenTable`. It calls the engine's C ABI ([`crates/opendoc-ffi`](../../crates/opendoc-ffi))
through the Foreign Function & Memory API ([JEP 454](https://openjdk.org/jeps/454), JDK 22+),
so there is no JNI glue and the jar has no dependencies. Licence: Apache-2.0.

**Not published to Maven Central.** That is a separate decision for the owner. Build and test
locally:

```sh
./bindings/java/build.sh     # cargo build -p opendoc-ffi --release, javac, jar, conformance test
```

Run with `--enable-native-access=ALL-UNNAMED`. Point the binding at the native library with
`-Dopendoc.library=/path/to/libopendoc_ffi.{dylib,so,dll}`, the environment variable
`OPENDOC_LIBRARY`, or `java.library.path`.

```java
try (OpenDocument doc = new OpenDocument()) {             // or new OpenDocument(inputStream)
    OpenParagraph p = doc.createParagraph();
    OpenRun r = p.createRun();
    r.setBold(true);                                        // before or after setText, as in POI
    r.setText("Hello");
    p.setAlignment(ParagraphAlignment.CENTER);
    OpenTable t = doc.createTable(2, 3);
    t.getRow(0).getCell(0).setText("Qty");
    t.createRow();
    try (OutputStream out = Files.newOutputStream(Path.of("out.docx"))) {
        doc.write(out);
    }
    doc.save(Path.of("out.pdf"));                           // Aspose-style: the format from the extension
    byte[] png = doc.renderPagePng(0, 150f);
    String svg = doc.renderPageSvg(0);                       // the same page as SVG, selectable text
    int pages = doc.getPageCount();
}
```

## Pictures, headers and footers, notes, sections

```java
OpenRun r = doc.createParagraph().createRun();
OpenPicture logo = r.addPicture(Files.newInputStream(Path.of("logo.png")),
        Document.PICTURE_TYPE_PNG, "logo.png", Units.toEMU(96), Units.toEMU(32)); // POI's signature
logo.setSize(144, 48);                                       // additions: setSize (points), setDescription,
logo.setDescription("Company logo");                         // replace(InputStream) and remove()

doc.createHeader(HeaderFooterType.DEFAULT).createParagraph().createRun().setText("Quarterly report");
OpenParagraph footer = doc.createFooter(HeaderFooterType.DEFAULT).createParagraph();
footer.createRun().setText("Page ");
footer.addField("page");                                     // numpages, date, time, filename, author
doc.createHeader(HeaderFooterType.FIRST)                     // turns the different first page on, as POI does
   .createParagraph().createRun().setText("Cover");

OpenNote note = doc.getParagraphs().get(0).addFootnote("Source"); // addEndnote too
note.setText("Source, p. 4");

OpenSection landscape = doc.addSection(SectionStart.NEXT_PAGE);
landscape.setOrientation("landscape");                      // the page turns, as in Word
landscape.setMargins(36, 36, 54, 54);                        // points
landscape.setColumns(2, 18);
landscape.setStart(SectionStart.ODD_PAGE);
```

`Document.PICTURE_TYPE_PNG` and `Units` above are POI's; here the format is read from the bytes,
so any `int` will do. POI exposes sections only as raw `CTSectPr`; `OpenSection` is the Page
Setup dialog's fields.

## What POI does not do

- PDF, ODT, HTML, Markdown and text output, laid out by the engine.
- Page images and page counts.
- Templates: `renderTemplate(Map)` fills docxtemplater-style tags.
- `replace` across every story, `acceptAllRevisions` and `compare`.
- A loss report on open (`getImportReport`) and on every save (`getLastSaveReport`).

Every edit is an engine operation, applied all or nothing, and a read-only document refuses
edits. POI's own documentation warns that its low-level API "may" write invalid files.

## Not built yet

- Run character styles and table styles cannot be set.
- Java 8 to 21 would need a JNI shim over the same C ABI (`docs/176` Q3). It is not built.
