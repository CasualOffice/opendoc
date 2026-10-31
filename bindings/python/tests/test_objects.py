# SPDX-License-Identifier: Apache-2.0
"""Pictures, headers and footers, notes and sections by edit, in python-docx's
names (docs/176 section 12.12)."""

import io

import pytest

from opendoc import WD_ORIENT, WD_SECTION, Document, Inches, OpenDocError, Pt


def png(dpi: float = 4.0) -> bytes:
    """A real PNG: a blank page drawn by the engine."""
    return Document().render_page_png(0, dpi=dpi)


def reopen(document: Document) -> Document:
    data, report = document.to_bytes("docx")
    assert report.is_lossless, report.losses
    return Document(data)


def test_add_picture_then_resize_describe_replace_and_delete(tmp_path):
    path = tmp_path / "logo.png"
    path.write_bytes(png())
    document = Document()
    shape = document.add_picture(str(path), width=Inches(1))
    assert len(document.inline_shapes) == 1
    assert document.inline_shapes[0].id == shape.id
    assert abs(shape.width.pt - 72) < 0.01
    run_shape = document.add_paragraph("Inline: ").add_run("x").add_picture(io.BytesIO(png()), height=Pt(20))
    assert abs(run_shape.height.pt - 20) < 0.01
    shape.width = Pt(144)
    shape.alt_text = "Logo"
    assert abs(shape.width.pt - 144) < 0.01
    assert shape.alt_text == "Logo"
    shape.replace(png(6.0))
    assert shape.alt_text == "Logo", "a replaced picture keeps its alt text"
    back = reopen(document)
    assert len(back.inline_shapes) == 2
    shape.delete()
    assert [s.id for s in document.inline_shapes] == [run_shape.id]
    with pytest.raises(OpenDocError):
        document.add_picture(io.BytesIO(b"not an image"))


def test_headers_and_footers_default_first_and_even_with_fields():
    document = Document()
    document.add_paragraph("Body")
    section = document.sections[0]
    section.header.paragraphs[0].text = "Quarterly report"
    footer = section.footer.paragraphs[0]
    footer.text = "Page "
    footer.add_field("page")
    section.different_first_page_header_footer = True
    section.first_page_header.paragraphs[0].text = "Cover"
    document.settings.odd_and_even_pages_header_footer = True
    even = section.even_page_header
    even.paragraphs[0].text = "Even"
    even.add_paragraph("Second line")
    for d in (document, reopen(document)):
        s = d.sections[0]
        assert s.header.text == "Quarterly report"
        assert s.first_page_header.text == "Cover"
        assert s.even_page_header.text == "Even\nSecond line"
        assert s.footer.text.startswith("Page ")
        assert s.different_first_page_header_footer
        assert d.settings.odd_and_even_pages_header_footer
    # A second section is linked to the first until unlinked.
    second = document.add_section(WD_SECTION.CONTINUOUS)
    assert second.header.is_linked_to_previous
    assert second.header.text == "Quarterly report"
    second.header.is_linked_to_previous = False
    second.header.paragraphs[0].text = "Part two"
    assert document.sections[0].header.text == "Quarterly report"
    assert document.sections[1].header.text == "Part two"


def test_footnotes_and_endnotes_are_added_edited_and_deleted():
    document = Document()
    p = document.add_paragraph("A claim.")
    note = p.add_footnote("Source A")
    end = p.add_endnote("Later")
    assert [n.text for n in document.footnotes] == ["Source A"]
    assert [n.text for n in document.endnotes] == ["Later"]
    note.text = "Source A, page 4"
    assert document.footnotes[0].text == "Source A, page 4"
    end.delete()
    assert document.endnotes == []
    assert [n.text for n in reopen(document).footnotes] == ["Source A, page 4"]


def test_sections_add_start_type_page_size_orientation_margins_columns():
    document = Document()
    document.add_paragraph("Portrait")
    section = document.add_section(WD_SECTION.NEW_PAGE)
    document.add_paragraph("Landscape")
    width, height = section.page_width, section.page_height
    section.orientation = WD_ORIENT.LANDSCAPE
    assert (section.page_width, section.page_height) == (height, width)
    section.left_margin = Inches(0.5)
    section.top_margin = Pt(36)
    section.columns = 2
    section.start_type = WD_SECTION.ODD_PAGE
    for d in (document, reopen(document)):
        s = d.sections[1]
        assert s.orientation == WD_ORIENT.LANDSCAPE
        assert abs(s.left_margin.pt - 36) < 0.01 and abs(s.top_margin.pt - 36) < 0.01
        assert s.columns == 2
        assert s.start_type == WD_SECTION.ODD_PAGE
        assert d.sections[0].orientation == WD_ORIENT.PORTRAIT
    with pytest.raises(OpenDocError):
        section.start_type = "sideways"
