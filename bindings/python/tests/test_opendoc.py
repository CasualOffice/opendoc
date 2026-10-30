# SPDX-License-Identifier: Apache-2.0
"""The Python package, driven as a python-docx user drives it."""

import io
import os
import pathlib

import pytest

import opendoc
from opendoc import Document, Inches, OpenDocError, Pt, RGBColor, Template, TemplateError

SAMPLE = pathlib.Path(__file__).resolve().parents[3] / "sample.docx"


def test_python_docx_quickstart_runs_unchanged_and_reads_back(tmp_path):
    # python-docx's documentation example, minus add_picture.
    document = Document()
    document.add_heading("Document Title", 0)
    p = document.add_paragraph("A plain paragraph having some ")
    p.add_run("bold").bold = True
    p.add_run(" and some ")
    p.add_run("italic.").italic = True
    document.add_heading("Heading, level 1", level=1)
    document.add_paragraph("Intense quote", style="Intense Quote" if "Intense Quote" in document.styles else "Quote")
    document.add_paragraph("first item in unordered list", style="List Bullet")
    document.add_paragraph("first item in ordered list", style="List Number")
    records = ((3, "101", "Spam"), (7, "422", "Eggs"), (4, "631", "Spam, spam, eggs, and spam"))
    table = document.add_table(rows=1, cols=3)
    hdr_cells = table.rows[0].cells
    hdr_cells[0].text = "Qty"
    hdr_cells[1].text = "Id"
    hdr_cells[2].text = "Desc"
    for qty, id_, desc in records:
        row_cells = table.add_row().cells
        row_cells[0].text = str(qty)
        row_cells[1].text = id_
        row_cells[2].text = desc
    document.add_page_break()
    out = tmp_path / "demo.docx"
    report = document.save(out)
    assert report.is_lossless, report.losses

    back = Document(out)
    texts = [p.text for p in back.paragraphs]
    assert "Document Title" in texts
    assert "A plain paragraph having some bold and some italic." in texts
    mixed = next(p for p in back.paragraphs if p.text.startswith("A plain"))
    assert [r.text for r in mixed.runs if r.bold] == ["bold"]
    assert [r.text for r in mixed.runs if r.italic] == ["italic."]
    assert next(p for p in back.paragraphs if p.text == "Heading, level 1").style == "heading 1"
    t = back.tables[0]
    assert [c.text for c in t.rows[3].cells] == ["4", "631", "Spam, spam, eggs, and spam"]
    assert t.cell(0, 2).text == "Desc"


def test_font_and_paragraph_setters_lower_to_engine_edits():
    document = Document()
    p = document.add_paragraph("Hello world")
    run = p.runs[0]
    run.font.size = Pt(14)
    run.font.name = "Arial"
    run.font.color.rgb = RGBColor(0x42, 0x24, 0xE9)
    p.alignment = "center"
    p.style = "Heading 2"
    p = document.paragraphs[-1]
    r = p.runs[0]
    assert (r.font.size.pt, r.font.name, str(r.font.color.rgb)) == (14.0, "Arial", "4224E9")
    assert p.alignment == "center"
    assert p.style == "heading 2"
    p.text = "Replaced"
    assert document.paragraphs[-1].text == "Replaced"


def test_open_from_path_bytes_and_stream_with_the_import_report():
    for source in (str(SAMPLE), SAMPLE.read_bytes(), io.BytesIO(SAMPLE.read_bytes())):
        document = Document(source)
        assert document.source_format == "docx"
        assert document.paragraphs
        report = document.import_report
        assert report.format == "docx"
        assert all(isinstance(f.loss, bool) for f in report.findings)
    section = Document(SAMPLE).sections[0]
    assert section.page_width > Inches(5)


def test_save_every_format_render_and_pdf(tmp_path):
    document = Document(SAMPLE)
    for ext in ("docx", "odt", "pdf", "html", "md", "txt"):
        report = document.save(tmp_path / f"out.{ext}")
        assert report.format == ext
        assert (tmp_path / f"out.{ext}").stat().st_size > 0
    assert (tmp_path / "out.pdf").read_bytes().startswith(b"%PDF-")
    png = document.render_page_png(0, dpi=24)
    assert png.startswith(b"\x89PNG")
    assert png == Document(SAMPLE).render_page_png(0, dpi=24)
    assert document.page_count > 1
    exact, _ = document.to_bytes("docx", mode="exact")
    assert exact == SAMPLE.read_bytes()


def test_comments_revisions_and_replace():
    document = Document(author="Grace")
    p = document.add_paragraph("Hello world")
    comment = document.add_comment(p.runs, text="Greeting?")
    assert comment.text == "Greeting?" and comment.author == "Grace"
    assert comment.anchor["start"] == 0 and comment.anchor["end"] == 11
    assert document.replace("world", "there") == 1
    assert "Hello there" in document.text
    document.edit([{"op": "suggestInsert", "paragraph": p.id, "offset": 11, "text": "!"}])
    assert any(r["kind"] == "insertion" for r in document.revisions)
    assert document.accept_all_revisions() is True
    assert document.revisions == []


def test_a_failed_edit_changes_nothing_and_names_its_code():
    document = Document(SAMPLE)
    before = document.text
    with pytest.raises(OpenDocError) as error:
        document.edit([{"op": "replaceAll", "find": "the", "replace": "THE"}, {"op": "deleteBlock", "id": "nope"}])
    assert error.value.code == "invalid-argument"
    assert document.text == before


def test_templates_fill_and_report_every_issue(tmp_path):
    template = opendoc.build(
        {
            "sections": [
                {
                    "children": [
                        {"type": "paragraph", "text": "Dear {name},"},
                        {"type": "paragraph", "text": "{#items}"},
                        {"type": "paragraph", "text": "- {title}"},
                        {"type": "paragraph", "text": "{/items}"},
                    ]
                }
            ]
        }
    )
    path = tmp_path / "t.docx"
    template.save(path)
    tpl = Template(path)
    assert ("items", "loop") in tpl.get_tags()
    tpl.render({"name": "Ada", "items": [{"title": "Pen"}, {"title": "Ink"}]})
    assert [p.text for p in tpl.document.paragraphs] == ["Dear Ada,", "- Pen", "- Ink"]
    broken = opendoc.build({"sections": [{"children": [{"type": "paragraph", "text": "Hi {name"}]}]})
    with pytest.raises(TemplateError) as error:
        broken.render({})
    assert [e["id"] for e in error.value.errors] == ["unclosed_tag"]


def test_core_properties_read_and_write():
    document = Document()
    document.core_properties.title = "Quarterly"
    document.core_properties.author = "Ada"
    assert document.core_properties.title == "Quarterly"
    assert document.core_properties.author == "Ada"
    # Read through the file's own fields, not the setter's mapping.
    built = opendoc.build({"creator": "Lin", "subject": "Sub", "sections": []})
    assert (built.core_properties.author, built.core_properties.subject) == ("Lin", "Sub")


def test_convert_and_formats(tmp_path):
    report = opendoc.convert(SAMPLE, tmp_path / "s.odt")
    assert report.format == "odt"
    assert Document(tmp_path / "s.odt").source_format == "odt"
    names = {f["name"] for f in opendoc.formats() if f["save"]}
    assert {"docx", "odt", "pdf", "md"} <= names
    assert opendoc.__version__
