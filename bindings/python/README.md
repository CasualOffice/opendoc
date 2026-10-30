# opendoc (Python)

OpenDoc for Python: a [python-docx](https://python-docx.readthedocs.io)-style API over the
OpenDoc engine. It creates, reads, modifies, templates, converts and renders DOCX and ODT.
It has its own page layout, writes real-text PDF and page PNGs, and reports anything a save
would lose. Licence: Apache-2.0. It is built with PyO3 and maturin.

**Not published to PyPI.** Publishing is a separate decision for the owner. Build a wheel
locally:

```sh
python -m venv .venv && . .venv/bin/activate
pip install maturin pytest
cd bindings/python
maturin develop --release        # or: maturin build --release (wheel in target/wheels)
pytest tests
```

## python-docx code runs as written

```python
from opendoc import Document, Pt, RGBColor

document = Document()                       # or Document("in.docx"), bytes, or a file object
document.add_heading("Document Title", 0)
p = document.add_paragraph("A plain paragraph having some ")
p.add_run("bold").bold = True
p.add_run(" and some ")
p.add_run("italic.").italic = True
document.add_paragraph("first item in unordered list", style="List Bullet")
table = document.add_table(rows=1, cols=3)
table.rows[0].cells[0].text = "Qty"
table.add_row().cells[0].text = "3"
p.runs[0].font.size = Pt(12)
p.runs[0].font.color.rgb = RGBColor(0x42, 0x24, 0xE9)
document.core_properties.title = "Demo"

report = document.save("demo.docx")         # a SaveReport: report.losses is empty when nothing was lost
```

## What python-docx cannot do

```python
document.save("demo.pdf")                   # laid out by the engine, real selectable text
document.save("demo.odt")                   # also .html, .md, .txt, .json
png = document.render_page_png(0, dpi=150)  # a page image
document.page_count                         # from the engine's own pagination
document.import_report.losses               # what the open could not carry
document.replace("ACME", "Contoso")         # every story, returns the count
document.add_comment(p.runs, "Check this", author="Ada")
document.accept_all_revisions()
document.compare(Document("v2.docx"))       # the editor's structural diff
```

Every edit is an engine operation, applied all or nothing. A read-only document refuses
edits. `Document.edit([...])` runs a list of JSON edit commands as one batch; it is the same
vocabulary the JavaScript package uses.

## Templates

```python
from opendoc import Template

tpl = Template("invoice.docx")              # docxtemplater tags: {name} {#items}...{/items} {^paid}...{/paid} {.}
tpl.render({"customer": "Contoso", "items": [{"name": "Pens", "qty": 10}], "paid": False})
tpl.save("invoice.pdf")
```

`delimiters=("{{", "}}")` changes the tag markers. The tags are docxtemplater's, not Jinja's:
loops are `{{#items}}…{{/items}}`, not `{% for %}`. An invalid template raises
`TemplateError`, whose `errors` lists every issue.

## Not built yet

- `add_picture` into an existing document raises, and says so. `opendoc.build({...})` with a
  docx-js-shaped spec writes pictures.
- Character styles on a run cannot be set.
- A table style other than Table Grid cannot be set.
- Headers and footers are read-only (`section.header.paragraphs`).
- `add_page_break()` adds an empty paragraph that starts a new page; python-docx adds a
  page-break run.
