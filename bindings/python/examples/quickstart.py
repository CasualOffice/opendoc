# SPDX-License-Identifier: Apache-2.0
"""python-docx code, then what python-docx cannot do: a PDF and a page image.

    python examples/quickstart.py out-dir

tests/test_opendoc.py runs this file, so it cannot rot.
"""
import pathlib
import sys

from opendoc import Document, Pt

out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "out")
out.mkdir(parents=True, exist_ok=True)

document = Document()
document.add_heading("Quarterly report", 0)
p = document.add_paragraph("Revenue grew ")
p.add_run("12%").bold = True
p.add_run(" quarter on quarter.")
p.runs[1].font.size = Pt(14)
table = document.add_table(rows=1, cols=2)
table.rows[0].cells[0].text = "Region"
table.rows[0].cells[1].text = "Q2"
table.add_row().cells[0].text = "North"

report = document.save(out / "report.docx")
print("report.docx:", "nothing lost" if report.is_lossless else report.losses)
document.save(out / "report.pdf")
(out / "page-1.png").write_bytes(document.render_page_png(0, dpi=96))
print(f"{document.page_count} page(s) laid out by the engine")
