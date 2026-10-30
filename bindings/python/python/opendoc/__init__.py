# SPDX-License-Identifier: Apache-2.0
"""OpenDoc for Python: a python-docx-style API over the OpenDoc engine.

    from opendoc import Document, Pt

    document = Document()                      # or Document("in.docx")
    document.add_heading("Document Title", 0)
    p = document.add_paragraph("A plain paragraph having some ")
    p.add_run("bold").bold = True
    p.add_run(" and some ")
    p.add_run("italic.").italic = True
    table = document.add_table(rows=1, cols=3)
    table.cell(0, 0).text = "Qty"
    report = document.save("demo.docx")        # a SaveReport: what was lost, if anything
    document.save("demo.pdf")                  # laid out by the engine, real text

What python-docx does not have: ``save`` to PDF/ODT/HTML/Markdown, page images
(``render_page_png``), ``page_count``, templates (``render``), a loss report on
every open and save, tracked changes and ``compare``. Every edit is an engine
operation, applied all or nothing (docs/176 section 12.8).
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from typing import IO, Any, Dict, Iterable, List, Optional, Sequence, Tuple, Union

from . import _opendoc

__all__ = [
    "Document",
    "Paragraph",
    "Run",
    "Table",
    "Row",
    "Cell",
    "Section",
    "HeaderFooter",
    "CoreProperties",
    "Comment",
    "Template",
    "SaveReport",
    "ImportReport",
    "Finding",
    "OpenDocError",
    "TemplateError",
    "Length",
    "Pt",
    "Inches",
    "Cm",
    "Mm",
    "Emu",
    "RGBColor",
    "WD_ALIGN_PARAGRAPH",
    "convert",
    "formats",
    "__version__",
]

__version__: str = _opendoc.VERSION

Source = Union[str, "os.PathLike[str]", bytes, bytearray, IO[bytes]]


# --- errors -----------------------------------------------------------------------


class OpenDocError(Exception):
    """Every SDK error. ``code`` is stable: ``open``, ``unsupported-format``,
    ``save``, ``edit``, ``invalid-argument``, ``build``, ``template``,
    ``render``, ``exhausted``."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


class TemplateError(OpenDocError):
    """A template that could not be filled. ``errors`` lists every issue, in
    docxtemplater's vocabulary (``id``: ``unclosed_tag``, ``unclosed_loop``,
    ``undefined_value``, ...)."""

    def __init__(self, message: str, errors: List[Dict[str, str]]) -> None:
        super().__init__("template", message)
        self.errors = errors


def _raise(error: Exception) -> None:
    text = str(error)
    code, _, message = text.partition(": ")
    if code == "template":
        summary, _, issues = message.partition(" | ")
        try:
            parsed = json.loads(issues) if issues else []
        except ValueError:
            parsed = []
        raise TemplateError(summary, parsed) from None
    if code and " " not in code:
        raise OpenDocError(code, message) from None
    raise OpenDocError("engine", text) from None


def _call(fn: Any, *args: Any, **kwargs: Any) -> Any:
    try:
        return fn(*args, **kwargs)
    except _opendoc.NativeError as error:  # type: ignore[attr-defined]
        _raise(error)


# --- units (python-docx's) ----------------------------------------------------------


class Length(int):
    """A length in EMU, as python-docx's ``Length``."""

    _EMU_PER_INCH = 914400
    _EMU_PER_PT = 12700

    @property
    def pt(self) -> float:
        return self / self._EMU_PER_PT

    @property
    def inches(self) -> float:
        return self / self._EMU_PER_INCH

    @property
    def cm(self) -> float:
        return self / 360000

    @property
    def mm(self) -> float:
        return self / 36000

    @property
    def emu(self) -> int:
        return int(self)

    @property
    def twips(self) -> int:
        return round(self / 635)


def Pt(points: float) -> Length:  # noqa: N802 - python-docx's name
    return Length(round(points * 12700))


def Inches(inches: float) -> Length:  # noqa: N802
    return Length(round(inches * 914400))


def Cm(cm: float) -> Length:  # noqa: N802
    return Length(round(cm * 360000))


def Mm(mm: float) -> Length:  # noqa: N802
    return Length(round(mm * 36000))


def Emu(emu: int) -> Length:  # noqa: N802
    return Length(emu)


class RGBColor(tuple):
    """An RGB colour, as python-docx's ``RGBColor(0x42, 0x24, 0xE9)``."""

    def __new__(cls, r: int, g: int, b: int) -> "RGBColor":
        return super().__new__(cls, (r, g, b))

    @classmethod
    def from_string(cls, rgb_hex: str) -> "RGBColor":
        h = rgb_hex.lstrip("#")
        return cls(int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))

    def __str__(self) -> str:
        return "%02X%02X%02X" % tuple(self)


class WD_ALIGN_PARAGRAPH:  # noqa: N801 - python-docx's name
    """Paragraph alignment, python-docx's enum names."""

    LEFT = "left"
    CENTER = "center"
    RIGHT = "right"
    JUSTIFY = "justify"


# --- reports ------------------------------------------------------------------------


@dataclass(frozen=True)
class Finding:
    feature: str
    occurrences: int
    representation: str
    retention: str
    loss: bool
    part: Optional[str] = None
    element: Optional[str] = None
    attribute: Optional[str] = None

    @classmethod
    def _of(cls, raw: Dict[str, Any]) -> "Finding":
        return cls(
            feature=raw["feature"],
            occurrences=raw["occurrences"],
            representation=raw["representation"],
            retention=raw["retention"],
            loss=raw["loss"],
            part=raw.get("part"),
            element=raw.get("element"),
            attribute=raw.get("attribute"),
        )


@dataclass(frozen=True)
class ImportReport:
    """What an open did not carry into the model, and what it repaired."""

    format: str
    findings: List[Finding]
    repairs: List[Dict[str, Any]]

    @property
    def losses(self) -> List[Finding]:
        return [f for f in self.findings if f.loss]


@dataclass(frozen=True)
class SaveReport:
    """What a save wrote and lost. ``losses`` empty means nothing was lost."""

    format: str
    mode: str
    lossy_by_nature: bool
    findings: List[Finding]
    losses: List[Finding]

    @property
    def is_lossless(self) -> bool:
        return not self.losses

    @classmethod
    def _of(cls, raw: str) -> "SaveReport":
        r = json.loads(raw)
        return cls(
            format=r["format"],
            mode=r["mode"],
            lossy_by_nature=r["lossyByNature"],
            findings=[Finding._of(f) for f in r["findings"]],
            losses=[Finding._of(f) for f in r["losses"]],
        )


# --- the document -------------------------------------------------------------------


def _read_source(source: Source) -> Tuple[bytes, Optional[str]]:
    if isinstance(source, (bytes, bytearray)):
        return bytes(source), None
    if hasattr(source, "read"):
        data = source.read()  # type: ignore[union-attr]
        return bytes(data), getattr(source, "name", None)
    path = os.fspath(source)  # type: ignore[arg-type]
    with open(path, "rb") as handle:
        return handle.read(), str(path)


class Document:
    """A word-processing document. ``Document()`` is a new blank document with
    Word's built-in styles; ``Document(path_or_bytes_or_file)`` opens DOCX,
    ODT, RTF, TXT or JSON."""

    def __init__(self, docx: Optional[Source] = None, *, author: Optional[str] = None, system_fonts: bool = False) -> None:
        if docx is None:
            native = _call(_opendoc.NativeDocument.create)
            if author:
                native.set_author(author)
        else:
            data, name = _read_source(docx)
            native = _call(_opendoc.NativeDocument.open, data, None, name, author, system_fonts)
        self._native = native
        self._version = 0
        self._cache: Optional[Tuple[int, List[Dict[str, Any]], Dict[str, Dict[str, Any]]]] = None
        self._nodes: Dict[str, Dict[str, Any]] = {}
        self._nodes_version = -1

    # -- internals --

    def _edit(self, *commands: Dict[str, Any]) -> List[Any]:
        clean = [{k: v for k, v in c.items() if v is not None} for c in commands]
        out = json.loads(_call(self._native.edit, json.dumps(clean)))
        self._version += 1
        return out["results"]

    def _body(self) -> Tuple[List[Dict[str, Any]], Dict[str, Dict[str, Any]]]:
        if self._cache is None or self._cache[0] != self._version:
            body = json.loads(self._native.body())
            index: Dict[str, Dict[str, Any]] = {}

            def visit(blocks: Iterable[Dict[str, Any]]) -> None:
                for block in blocks:
                    index[block["id"]] = block
                    if block["type"] == "table":
                        for row in block["rows"]:
                            index[row["id"]] = row
                            for cell in row["cells"]:
                                index[cell["id"]] = cell
                                visit(cell["blocks"])
                    elif block["type"] == "contentControl":
                        visit(block["blocks"])

            visit(body)
            self._cache = (self._version, body, index)
        return self._cache[1], self._cache[2]

    def _node(self, node_id: str) -> Dict[str, Any]:
        # One paragraph or table, projected alone: a python-docx loop
        # (`add_paragraph`, then `add_run(...).bold = True`) must not re-project
        # the whole body per call, which made it quadratic.
        if self._nodes_version != self._version:
            self._nodes = {}
            self._nodes_version = self._version
        node = self._nodes.get(node_id)
        if node is None:
            raw = self._native.paragraph(node_id) or self._native.table(node_id)
            if raw is None:
                raise OpenDocError("invalid-argument", f"node {node_id} is no longer in the document")
            node = json.loads(raw)
            self._nodes[node_id] = node
        return node

    def _table(self, table_id: str) -> Dict[str, Any]:
        # Tables are looked up as tables: trying the paragraph locator first
        # cost every table access a cold miss that walks the document.
        if self._nodes_version != self._version:
            self._nodes = {}
            self._nodes_version = self._version
        node = self._nodes.get(table_id)
        if node is None:
            raw = self._native.table(table_id)
            if raw is None:
                raise OpenDocError("invalid-argument", f"table {table_id} is no longer in the document")
            node = json.loads(raw)
            self._nodes[table_id] = node
        return node

    # -- read (python-docx) --

    @property
    def paragraphs(self) -> List["Paragraph"]:
        return [Paragraph(self, b["id"]) for b in self._body()[0] if b["type"] == "paragraph"]

    @property
    def tables(self) -> List["Table"]:
        return [Table(self, b["id"]) for b in self._body()[0] if b["type"] == "table"]

    @property
    def sections(self) -> List["Section"]:
        return [Section(self, s) for s in json.loads(self._native.sections())]

    @property
    def styles(self) -> Dict[str, Dict[str, Any]]:
        """Style definitions by display name."""
        return {s.get("name") or s["id"]: s for s in json.loads(self._native.styles())}

    @property
    def core_properties(self) -> "CoreProperties":
        return CoreProperties(self)

    @property
    def comments(self) -> List["Comment"]:
        return [Comment(c) for c in json.loads(self._native.comments())]

    @property
    def text(self) -> str:
        return self._native.text()

    # -- extras python-docx lacks --

    @property
    def import_report(self) -> ImportReport:
        r = json.loads(self._native.import_report())
        return ImportReport(r["format"], [Finding._of(f) for f in r["findings"]], r["repairs"])

    @property
    def revisions(self) -> List[Dict[str, Any]]:
        return json.loads(self._native.revisions())

    @property
    def notes(self) -> List[Dict[str, Any]]:
        return json.loads(self._native.notes())

    @property
    def headers_footers(self) -> List[Dict[str, Any]]:
        return json.loads(self._native.headers_footers())

    @property
    def media(self) -> List[Dict[str, Any]]:
        return json.loads(self._native.media())

    @property
    def source_format(self) -> Optional[str]:
        return self._native.source_format()

    @property
    def is_modified(self) -> bool:
        return self._native.is_modified()

    # -- modify (python-docx) --

    def add_paragraph(self, text: str = "", style: Optional[str] = None) -> "Paragraph":
        (pid,) = self._edit({"op": "insertParagraph", "text": text, "style": style})
        return Paragraph(self, pid)

    def add_heading(self, text: str = "", level: int = 1) -> "Paragraph":
        if not 0 <= level <= 9:
            raise ValueError("level must be in range 0-9, got %d" % level)
        return self.add_paragraph(text, "Title" if level == 0 else "Heading %d" % level)

    def add_page_break(self) -> "Paragraph":
        """An empty paragraph that starts a new page (python-docx writes a
        page-break run; this sets the paragraph's page-break-before)."""
        p = self.add_paragraph("")
        self._edit({"op": "setPageBreakBefore", "paragraph": p.id, "on": True})
        return p

    def add_table(self, rows: int, cols: int, style: Optional[str] = None) -> "Table":
        (tid,) = self._edit({"op": "insertTable", "rows": rows, "columns": cols})
        if style is not None and style != "Table Grid":
            raise OpenDocError("invalid-argument", "add_table applies 'Table Grid'; other table styles are not settable by edit yet")
        return Table(self, tid)

    def add_picture(self, *_: Any, **__: Any) -> None:
        raise OpenDocError(
            "invalid-argument",
            "inserting a picture into an existing document is not built yet (docs/176 section 12.3); build the document with pictures through opendoc.build",
        )

    def add_comment(self, runs: Union["Run", Sequence["Run"]], text: str, author: Optional[str] = None, initials: Optional[str] = None) -> "Comment":
        """python-docx 1.2's ``add_comment``: anchored on a run or a contiguous
        run sequence in one paragraph."""
        runs = [runs] if isinstance(runs, Run) else list(runs)
        if not runs or len({r._paragraph for r in runs}) != 1:
            raise OpenDocError("invalid-argument", "a comment's runs must be in one paragraph")
        if author:
            self._native.set_author(author)
        start, end = min(r._start for r in runs), max(r._end for r in runs)
        (cid,) = self._edit({"op": "addComment", "paragraph": runs[0]._paragraph, "start": start, "end": end, "text": text})
        for comment in self.comments:
            if comment.id == cid:
                return comment
        raise OpenDocError("edit", "the comment was not created")

    def replace(self, find: str, replace: str) -> int:
        """Replaces every occurrence in every story; returns the count (Aspose's ``Range.Replace``)."""
        (count,) = self._edit({"op": "replaceAll", "find": find, "replace": replace})
        return int(count)

    def accept_all_revisions(self) -> bool:
        (changed,) = self._edit({"op": "acceptAllRevisions"})
        return bool(changed)

    def reject_all_revisions(self) -> bool:
        (changed,) = self._edit({"op": "rejectAllRevisions"})
        return bool(changed)

    def edit(self, commands: List[Dict[str, Any]]) -> List[Any]:
        """Runs edit commands (the JSON vocabulary every binding shares) as one atomic batch."""
        return self._edit(*commands)

    # -- templates --

    def render(self, context: Dict[str, Any], *, delimiters: Tuple[str, str] = ("{", "}"), null_value: str = "", strict: bool = False) -> "Document":
        """Fills docxtemplater-style tags: ``{name}``, ``{#items}...{/items}``,
        ``{^flag}...{/flag}``, ``{.}``. Raises ``TemplateError`` listing every issue."""
        _call(self._native.render_template, json.dumps(context, default=str), delimiters[0], delimiters[1], null_value, strict)
        self._version += 1
        return self

    def template_tags(self, delimiters: Tuple[str, str] = ("{", "}")) -> List[Tuple[str, str]]:
        return [tuple(t) for t in json.loads(self._native.template_tags(delimiters[0], delimiters[1]))]  # type: ignore[misc]

    # -- save, render --

    def save(self, path_or_stream: Union[str, "os.PathLike[str]", IO[bytes]], *, format: Optional[str] = None, mode: Optional[str] = None) -> SaveReport:
        """Saves; the format comes from the file extension unless given.
        ``mode`` (same format only): ``exact``, ``preserve`` (default), ``semantic``."""
        if format is None:
            name = getattr(path_or_stream, "name", None) if hasattr(path_or_stream, "write") else os.fspath(path_or_stream)  # type: ignore[arg-type]
            format = str(name).rsplit(".", 1)[-1].lower() if name and "." in str(name) else "docx"
        data, report = _call(self._native.save, format, mode)
        if hasattr(path_or_stream, "write"):
            path_or_stream.write(data)  # type: ignore[union-attr]
        else:
            with open(os.fspath(path_or_stream), "wb") as handle:  # type: ignore[arg-type]
                handle.write(data)
        return SaveReport._of(report)

    def to_bytes(self, format: str = "docx", mode: Optional[str] = None) -> Tuple[bytes, SaveReport]:
        data, report = _call(self._native.save, format, mode)
        return data, SaveReport._of(report)

    @property
    def page_count(self) -> int:
        return self._native.page_count()

    def page_sizes(self) -> List[Tuple[float, float]]:
        return self._native.page_sizes()

    def page_text(self, index: int) -> str:
        return _call(self._native.page_text, index)

    def render_page_png(self, index: int, dpi: float = 96.0) -> bytes:
        return _call(self._native.render_page_png, index, dpi)

    def render_page(self, index: int, dpi: float = 96.0) -> Tuple[int, int, bytes]:
        return _call(self._native.render_page, index, dpi)

    def to_pdf(self) -> bytes:
        return _call(self._native.to_pdf)

    def outline(self) -> List[Tuple[int, str, Optional[int]]]:
        return self._native.outline()

    def compare(self, newer: "Document") -> Dict[str, Any]:
        return json.loads(_call(self._native.compare, newer._native))


class Paragraph:
    """A paragraph (python-docx ``Paragraph``)."""

    def __init__(self, document: Document, paragraph_id: str) -> None:
        self._document = document
        self.id = paragraph_id

    def _view(self) -> Dict[str, Any]:
        return self._document._node(self.id)

    @property
    def text(self) -> str:
        return self._view()["text"]

    @text.setter
    def text(self, value: str) -> None:
        length = len(self.text)
        commands: List[Dict[str, Any]] = []
        if length:
            commands.append({"op": "deleteText", "paragraph": self.id, "start": 0, "end": length})
        if value:
            commands.append({"op": "insertText", "paragraph": self.id, "offset": 0, "text": value})
        if commands:
            self._document._edit(*commands)

    @property
    def style(self) -> Optional[str]:
        return self._view().get("style")

    @style.setter
    def style(self, name: str) -> None:
        self._document._edit({"op": "setParagraphStyle", "paragraph": self.id, "style": name})

    @property
    def alignment(self) -> Optional[str]:
        return {"start": "left", "end": "right"}.get(self._view().get("alignment", ""), self._view().get("alignment"))

    @alignment.setter
    def alignment(self, value: str) -> None:
        self._document._edit({"op": "setAlignment", "paragraph": self.id, "alignment": value})

    @property
    def heading_level(self) -> Optional[int]:
        return self._view().get("headingLevel")

    @property
    def runs(self) -> List["Run"]:
        out: List[Run] = []
        offset = 0
        for raw in self._view()["runs"]:
            length = len(raw["text"])
            out.append(Run(self._document, self.id, offset, offset + length, raw))
            offset += length
        return out

    def add_run(self, text: Optional[str] = None, style: Optional[str] = None) -> "Run":
        if style is not None:
            raise OpenDocError("invalid-argument", "a character style on a run is not settable by edit yet; set bold/italic/font instead")
        start = len(self.text)
        text = text or ""
        if text:
            self._document._edit({"op": "appendRun", "paragraph": self.id, "text": text})
        return Run(self._document, self.id, start, start + len(text), {"text": text})

    def insert_paragraph_before(self, text: str = "", style: Optional[str] = None) -> "Paragraph":
        (pid,) = self._document._edit({"op": "insertParagraph", "before": self.id, "text": text, "style": style})
        return Paragraph(self._document, pid)

    def __repr__(self) -> str:
        return "<opendoc.Paragraph %r>" % self.text[:40]


class _Font:
    def __init__(self, run: "Run") -> None:
        self._run = run

    @property
    def bold(self) -> Optional[bool]:
        return self._run._get("bold")

    @bold.setter
    def bold(self, value: Optional[bool]) -> None:
        self._run._set(bold=bool(value))

    @property
    def italic(self) -> Optional[bool]:
        return self._run._get("italic")

    @italic.setter
    def italic(self, value: Optional[bool]) -> None:
        self._run._set(italic=bool(value))

    @property
    def underline(self) -> Optional[bool]:
        return self._run._get("underline")

    @underline.setter
    def underline(self, value: Optional[bool]) -> None:
        self._run._set(underline=bool(value))

    @property
    def strike(self) -> Optional[bool]:
        return self._run._get("strike")

    @strike.setter
    def strike(self, value: Optional[bool]) -> None:
        self._run._set(strike=bool(value))

    @property
    def size(self) -> Optional[Length]:
        size = self._run._get("sizePt")
        return None if size is None else Pt(size)

    @size.setter
    def size(self, value: Union[Length, float]) -> None:
        points = value.pt if isinstance(value, Length) else float(value)
        self._run._set(sizePt=points)

    @property
    def name(self) -> Optional[str]:
        return self._run._get("font")

    @name.setter
    def name(self, value: str) -> None:
        self._run._set(font=value)

    @property
    def color(self) -> "_Color":
        return _Color(self._run)

    @property
    def highlight_color(self) -> Optional[str]:
        return self._run._get("highlight")

    @highlight_color.setter
    def highlight_color(self, value: str) -> None:
        self._run._set(highlight=value)

    @property
    def superscript(self) -> bool:
        return self._run._get("verticalAlign") == "superscript"

    @superscript.setter
    def superscript(self, value: bool) -> None:
        self._run._set(verticalAlign="superscript" if value else "baseline")

    @property
    def subscript(self) -> bool:
        return self._run._get("verticalAlign") == "subscript"

    @subscript.setter
    def subscript(self, value: bool) -> None:
        self._run._set(verticalAlign="subscript" if value else "baseline")


class _Color:
    def __init__(self, run: "Run") -> None:
        self._run = run

    @property
    def rgb(self) -> Optional[RGBColor]:
        value = self._run._get("color")
        return None if value is None else RGBColor.from_string(value)

    @rgb.setter
    def rgb(self, value: Union[RGBColor, str]) -> None:
        self._run._set(color=str(value))


class Run:
    """A run of text (python-docx ``Run``). Its formatting setters format
    exactly the characters it covers."""

    def __init__(self, document: Document, paragraph_id: str, start: int, end: int, raw: Dict[str, Any]) -> None:
        self._document = document
        self._paragraph = paragraph_id
        self._start = start
        self._end = end
        self._raw = raw

    def _get(self, key: str) -> Any:
        # The view's runs may have split since this object was made; read the
        # first run that starts at or covers this one's start.
        offset = 0
        for raw in self._document._node(self._paragraph)["runs"]:
            length = len(raw["text"])
            if offset <= self._start < offset + max(length, 1):
                return raw.get(key)
            offset += length
        return self._raw.get(key)

    def _set(self, **fmt: Any) -> None:
        if self._end > self._start:
            self._document._edit({"op": "setRunFormat", "paragraph": self._paragraph, "start": self._start, "end": self._end, "format": fmt})

    @property
    def text(self) -> str:
        return self._document._node(self._paragraph)["text"][self._start : self._end]

    @text.setter
    def text(self, value: str) -> None:
        commands: List[Dict[str, Any]] = []
        if self._end > self._start:
            commands.append({"op": "deleteText", "paragraph": self._paragraph, "start": self._start, "end": self._end})
        if value:
            commands.append({"op": "insertText", "paragraph": self._paragraph, "offset": self._start, "text": value})
        if commands:
            self._document._edit(*commands)
        self._end = self._start + len(value)

    @property
    def font(self) -> _Font:
        return _Font(self)

    bold = property(lambda self: self.font.bold, lambda self, v: setattr(self.font, "bold", v))
    italic = property(lambda self: self.font.italic, lambda self, v: setattr(self.font, "italic", v))
    underline = property(lambda self: self.font.underline, lambda self, v: setattr(self.font, "underline", v))

    @property
    def hyperlink(self) -> Optional[str]:
        return self._get("hyperlink")

    def __repr__(self) -> str:
        return "<opendoc.Run %r>" % self.text[:40]


class Cell:
    """A table cell (python-docx ``_Cell``). Setting ``text`` is one engine
    edit and reads nothing; an index outside the table is refused by the
    engine (``OpenDocError``), as python-docx's ``IndexError`` would be."""

    def __init__(self, document: Document, table_id: str, row: int, column: int) -> None:
        self._document = document
        self._table = table_id
        self._row = row
        self._column = column

    def _view(self) -> Dict[str, Any]:
        rows = self._document._table(self._table)["rows"]
        if not (0 <= self._row < len(rows) and 0 <= self._column < len(rows[self._row]["cells"])):
            raise IndexError("cell (%d, %d) is outside the table" % (self._row, self._column))
        return rows[self._row]["cells"][self._column]

    @property
    def text(self) -> str:
        return "\n".join(b.get("text", "") for b in self._view()["blocks"])

    @text.setter
    def text(self, value: str) -> None:
        self._document._edit({"op": "setCellText", "table": self._table, "row": self._row, "column": self._column, "text": value})

    @property
    def paragraphs(self) -> List[Paragraph]:
        return [Paragraph(self._document, b["id"]) for b in self._view()["blocks"] if b["type"] == "paragraph"]


class Row:
    def __init__(self, document: Document, table_id: str, index: int, count: int) -> None:
        self._document = document
        self._table = table_id
        self._index = index
        self._count = count

    @property
    def cells(self) -> List[Cell]:
        return [Cell(self._document, self._table, self._index, c) for c in range(self._count)]


class Table:
    """A table (python-docx ``Table``).

    Its shape (row count, cells per row) is read once and kept up to date by
    ``add_row``: a python-docx loop (``table.add_row().cells[0].text = ...``)
    must not re-project the whole table per cell, which made it O(rows²).
    """

    def __init__(self, document: Document, table_id: str) -> None:
        self._document = document
        self.id = table_id
        self._shape: Optional[List[int]] = None

    def _view(self) -> Dict[str, Any]:
        return self._document._table(self.id)

    def _cells_per_row(self) -> List[int]:
        if self._shape is None:
            self._shape = [len(r["cells"]) for r in self._view()["rows"]]
        return self._shape

    @property
    def rows(self) -> List[Row]:
        self._shape = None  # a row may have been added or removed by another call
        return [Row(self._document, self.id, r, n) for r, n in enumerate(self._cells_per_row())]

    @property
    def style(self) -> Optional[str]:
        return self._view().get("style")

    def cell(self, row_idx: int, col_idx: int) -> Cell:
        shape = self._cells_per_row()
        if not (0 <= row_idx < len(shape) and 0 <= col_idx < shape[row_idx]):
            self._shape = None
            shape = self._cells_per_row()
            if not (0 <= row_idx < len(shape) and 0 <= col_idx < shape[row_idx]):
                raise IndexError("cell (%d, %d) is outside the table" % (row_idx, col_idx))
        return Cell(self._document, self.id, row_idx, col_idx)

    def add_row(self) -> Row:
        shape = self._cells_per_row()
        self._document._edit({"op": "addRow", "table": self.id})
        # The engine copies the last row's layout.
        shape.append(shape[-1] if shape else 0)
        return Row(self._document, self.id, len(shape) - 1, shape[-1])


@dataclass(frozen=True)
class HeaderFooter:
    """A header or footer, read-only (``paragraphs``, ``text``)."""

    id: str
    text: str
    blocks: List[Dict[str, Any]] = field(default_factory=list)

    @property
    def paragraphs(self) -> List[str]:
        return [b.get("text", "") for b in self.blocks if b.get("type") == "paragraph"]


class Section:
    """A section's page setup (python-docx ``Section``), read-only."""

    def __init__(self, document: Document, raw: Dict[str, Any]) -> None:
        self._document = document
        self._raw = raw

    page_width = property(lambda self: Pt(self._raw["pageWidthPt"]))
    page_height = property(lambda self: Pt(self._raw["pageHeightPt"]))
    orientation = property(lambda self: self._raw["orientation"])
    left_margin = property(lambda self: Pt(self._raw["marginsPt"]["left"]))
    right_margin = property(lambda self: Pt(self._raw["marginsPt"]["right"]))
    top_margin = property(lambda self: Pt(self._raw["marginsPt"]["top"]))
    bottom_margin = property(lambda self: Pt(self._raw["marginsPt"]["bottom"]))

    def _story(self, part: str, kind: str = "default") -> Optional[HeaderFooter]:
        refs = self._raw["headers" if part == "header" else "footers"]
        wanted = next((r["id"] for r in refs if r["kind"] == kind), None)
        for story in self._document.headers_footers:
            if story["id"] == wanted and story["part"] == part:
                return HeaderFooter(story["id"], story["text"], story["blocks"])
        return None

    @property
    def header(self) -> Optional[HeaderFooter]:
        return self._story("header")

    @property
    def footer(self) -> Optional[HeaderFooter]:
        return self._story("footer")

    @property
    def first_page_header(self) -> Optional[HeaderFooter]:
        return self._story("header", "first")


class CoreProperties:
    """python-docx's ``core_properties``: get and set."""

    _FIELDS = {
        "title": "title",
        "subject": "subject",
        "author": "creator",
        "keywords": "keywords",
        "comments": "description",
        "last_modified_by": "lastModifiedBy",
        "category": "category",
    }

    def __init__(self, document: Document) -> None:
        object.__setattr__(self, "_document", document)

    def __getattr__(self, name: str) -> Any:
        props = json.loads(self._document._native.properties())
        if name in self._FIELDS:
            return props.get(self._FIELDS[name]) or ""
        if name in ("revision", "created", "modified"):
            return props.get(name)
        raise AttributeError(name)

    def __setattr__(self, name: str, value: Any) -> None:
        if name not in self._FIELDS:
            raise AttributeError("%s is not a settable core property" % name)
        self._document._edit({"op": "setProperties", "properties": {self._FIELDS[name]: str(value)}})


@dataclass(frozen=True)
class Comment:
    id: str
    text: str
    author: Optional[str]
    initials: Optional[str]
    date: Optional[str]
    resolved: bool
    anchor: Optional[Dict[str, Any]]

    def __init__(self, raw: Dict[str, Any]) -> None:
        for key, value in (
            ("id", raw["id"]),
            ("text", raw["text"]),
            ("author", raw.get("author")),
            ("initials", raw.get("initials")),
            ("date", raw.get("date")),
            ("resolved", raw["resolved"]),
            ("anchor", raw.get("anchor")),
        ):
            object.__setattr__(self, key, value)


class Template:
    """A template filled with docxtemplater-style tags (``{name}``,
    ``{#items}...{/items}``, ``{^flag}...{/flag}``, ``{.}``)::

        tpl = Template("invoice.docx")
        tpl.render({"customer": "Contoso", "lines": [...]})
        tpl.save("invoice.pdf")
    """

    def __init__(self, source: Source, *, delimiters: Tuple[str, str] = ("{", "}"), null_value: str = "", strict: bool = False) -> None:
        self.document = Document(source)
        self._options = dict(delimiters=delimiters, null_value=null_value, strict=strict)

    def get_tags(self) -> List[Tuple[str, str]]:
        return self.document.template_tags(self._options["delimiters"])  # type: ignore[arg-type]

    def render(self, context: Dict[str, Any]) -> "Template":
        self.document.render(context, **self._options)  # type: ignore[arg-type]
        return self

    def save(self, path_or_stream: Union[str, "os.PathLike[str]", IO[bytes]], **kwargs: Any) -> SaveReport:
        return self.document.save(path_or_stream, **kwargs)


def build(spec: Dict[str, Any], media: Sequence[bytes] = ()) -> Document:
    """Builds a document from a docx-js-shaped spec dict (``sections``,
    ``children``, ``TextRun`` options...), the same JSON the JS builder sends."""
    native = _call(_opendoc.NativeDocument.from_spec, json.dumps(spec), [bytes(m) for m in media])
    doc = Document.__new__(Document)
    doc._native = native
    doc._version = 0
    doc._cache = None
    doc._nodes = {}
    doc._nodes_version = -1
    return doc


def convert(source: Source, target: Union[str, "os.PathLike[str]"], *, to: Optional[str] = None) -> SaveReport:
    """Converts a file (or bytes) to ``target``; the format comes from its extension."""
    data, _ = _read_source(source)
    fmt = to or os.fspath(target).rsplit(".", 1)[-1].lower()
    out, report = _call(_opendoc.convert, data, fmt)
    with open(os.fspath(target), "wb") as handle:
        handle.write(out)
    return SaveReport._of(report)


def formats() -> List[Dict[str, Any]]:
    return json.loads(_opendoc.formats())

