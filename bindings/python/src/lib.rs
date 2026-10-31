// SPDX-License-Identifier: Apache-2.0

//! `opendoc._opendoc`: the native half of the Python package. It is thin on
//! purpose: every structured value crosses as the Rust read view's JSON, and
//! `python/opendoc/__init__.py` turns it into python-docx-shaped objects, so
//! the Python and JavaScript bindings read one serialisation.

use opendoc::{
    Document, DocumentSpec, EditCommand, Format, OpenOptions, RenderOptions, SaveMode,
    TemplateOptions, Undo,
};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

create_exception!(_opendoc, NativeError, PyException);

fn err(error: opendoc::Error) -> PyErr {
    NativeError::new_err(error.to_string())
}

fn invalid(message: String) -> PyErr {
    NativeError::new_err(format!("invalid-argument: {message}"))
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn format(name: &str) -> PyResult<Format> {
    name.parse().map_err(err)
}

/// An open document.
#[pyclass(unsendable, module = "opendoc._opendoc")]
struct NativeDocument {
    inner: Document,
}

#[pymethods]
impl NativeDocument {
    #[staticmethod]
    #[pyo3(signature = (data, format=None, file_name=None, author=None, system_fonts=false))]
    fn open(
        data: &[u8],
        format: Option<&str>,
        file_name: Option<String>,
        author: Option<String>,
        system_fonts: bool,
    ) -> PyResult<Self> {
        let options = OpenOptions {
            format: format.map(self::format).transpose()?,
            file_name,
            author,
            render: RenderOptions {
                system_fonts,
                fonts: Vec::new(),
            },
        };
        Document::open_with(data, &options)
            .map(|inner| Self { inner })
            .map_err(err)
    }

    #[staticmethod]
    fn create() -> PyResult<Self> {
        Document::create().map(|inner| Self { inner }).map_err(err)
    }

    #[staticmethod]
    #[pyo3(signature = (spec_json, media=Vec::new()))]
    fn from_spec(spec_json: &str, media: Vec<Vec<u8>>) -> PyResult<Self> {
        Document::from_spec_json(spec_json, &media)
            .map(|inner| Self { inner })
            .map_err(err)
    }

    fn source_format(&self) -> Option<&'static str> {
        self.inner.source_format().map(Format::name)
    }

    fn is_modified(&self) -> bool {
        self.inner.is_modified()
    }

    fn set_author(&mut self, author: &str) {
        self.inner.set_author(author);
    }

    fn import_report(&self) -> String {
        json(self.inner.import_report())
    }

    fn body(&self) -> String {
        json(&self.inner.body())
    }

    /// One paragraph's view, JSON, or `None`.
    fn paragraph(&self, id: &str) -> Option<String> {
        self.inner.paragraph(id).map(|p| json(&p))
    }

    /// One table's view, JSON, or `None`.
    fn table(&self, id: &str) -> Option<String> {
        self.inner.table(id).map(|t| json(&t))
    }

    fn text(&self) -> String {
        self.inner.text()
    }

    fn sections(&self) -> String {
        json(&self.inner.sections())
    }

    fn styles(&self) -> String {
        json(&self.inner.styles())
    }

    fn comments(&self) -> String {
        json(&self.inner.comments())
    }

    fn revisions(&self) -> String {
        json(&self.inner.revisions())
    }

    fn headers_footers(&self) -> String {
        json(&self.inner.headers_footers())
    }

    fn notes(&self) -> String {
        json(&self.inner.notes())
    }

    fn media(&self) -> String {
        json(&self.inner.media())
    }

    fn media_bytes<'py>(&self, py: Python<'py>, part: &str) -> Option<Bound<'py, PyBytes>> {
        self.inner.media_bytes(part).map(|b| PyBytes::new(py, b))
    }

    fn properties(&self) -> String {
        json(&self.inner.properties())
    }

    /// Runs a JSON list of edit commands as one batch; returns
    /// `{"results": [...], "undo": "..."}`.
    fn edit(&mut self, commands_json: &str) -> PyResult<String> {
        let commands: Vec<EditCommand> = serde_json::from_str(commands_json)
            .map_err(|e| invalid(format!("not a list of edits: {e}")))?;
        let (results, undo) = self.inner.edit_commands(&commands).map_err(err)?;
        Ok(serde_json::json!({ "results": results, "undo": undo.to_json() }).to_string())
    }

    fn undo(&mut self, undo_json: &str) -> PyResult<String> {
        let undo = Undo::from_json(undo_json).map_err(err)?;
        self.inner.undo(&undo).map(|r| r.to_json()).map_err(err)
    }

    #[pyo3(signature = (data_json, start="{", end="}", null_value="", strict=false))]
    fn render_template(
        &mut self,
        data_json: &str,
        start: &str,
        end: &str,
        null_value: &str,
        strict: bool,
    ) -> PyResult<()> {
        let data: serde_json::Value =
            serde_json::from_str(data_json).map_err(|e| invalid(format!("data: {e}")))?;
        let options = TemplateOptions {
            start: start.to_owned(),
            end: end.to_owned(),
            null_value: null_value.to_owned(),
            strict,
        };
        self.inner.render_template(&data, &options).map_err(err)
    }

    #[pyo3(signature = (start="{", end="}"))]
    fn template_tags(&self, start: &str, end: &str) -> String {
        let options = TemplateOptions {
            start: start.to_owned(),
            end: end.to_owned(),
            ..TemplateOptions::default()
        };
        json(&self.inner.template_tags(&options))
    }

    /// Returns `(bytes, report_json)`.
    #[pyo3(signature = (fmt="docx", mode=None))]
    fn save<'py>(
        &self,
        py: Python<'py>,
        fmt: &str,
        mode: Option<&str>,
    ) -> PyResult<(Bound<'py, PyBytes>, String)> {
        let mode = match mode {
            Some(mode) => mode.parse().map_err(err)?,
            None => SaveMode::default(),
        };
        let (bytes, report) = self.inner.save(format(fmt)?, mode).map_err(err)?;
        Ok((PyBytes::new(py, &bytes), json(&report)))
    }

    fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    fn page_sizes(&self) -> Vec<(f64, f64)> {
        self.inner
            .page_sizes()
            .into_iter()
            .map(|s| (s.width_pt, s.height_pt))
            .collect()
    }

    fn page_text(&self, index: usize) -> PyResult<String> {
        self.inner.page_text(index).map_err(err)
    }

    #[pyo3(signature = (index, dpi=96.0))]
    fn render_page_png<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        dpi: f32,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let png = self.inner.render_page_png(index, dpi).map_err(err)?;
        Ok(PyBytes::new(py, &png))
    }

    fn render_page_svg(&self, index: usize) -> PyResult<String> {
        self.inner.render_page_svg(index).map_err(err)
    }

    /// Returns `(width, height, rgba)`.
    #[pyo3(signature = (index, dpi=96.0))]
    fn render_page<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        dpi: f32,
    ) -> PyResult<(u32, u32, Bound<'py, PyBytes>)> {
        let image = self.inner.render_page(index, dpi).map_err(err)?;
        Ok((image.width, image.height, PyBytes::new(py, &image.rgba)))
    }

    fn to_pdf<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let pdf = self.inner.to_pdf().map_err(err)?;
        Ok(PyBytes::new(py, &pdf))
    }

    fn outline(&self) -> Vec<(u8, String, Option<usize>)> {
        self.inner.outline()
    }

    fn compare(&self, newer: &NativeDocument) -> PyResult<String> {
        self.inner
            .compare(&newer.inner)
            .map(|v| v.to_string())
            .map_err(err)
    }
}

/// Converts bytes to another format; returns `(bytes, report_json)`.
#[pyfunction]
fn convert<'py>(py: Python<'py>, data: &[u8], to: &str) -> PyResult<(Bound<'py, PyBytes>, String)> {
    let (bytes, report) = opendoc::convert(data, format(to)?).map_err(err)?;
    Ok((PyBytes::new(py, &bytes), json(&report)))
}

/// The DOCX package for a docx-js-shaped spec, without opening it.
#[pyfunction]
#[pyo3(signature = (spec_json, media=Vec::new()))]
fn build_docx<'py>(
    py: Python<'py>,
    spec_json: &str,
    media: Vec<Vec<u8>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let spec: DocumentSpec = serde_json::from_str(spec_json)
        .map_err(|e| NativeError::new_err(format!("build: not a document spec: {e}")))?;
    let bytes = opendoc::builder::build_docx(&spec, &media).map_err(err)?;
    Ok(PyBytes::new(py, &bytes))
}

/// Every format and its directions, JSON.
#[pyfunction]
fn formats() -> String {
    let list: Vec<_> = Format::ALL
        .into_iter()
        .map(
            |f| serde_json::json!({ "name": f.name(), "open": f.can_open(), "save": f.can_save() }),
        )
        .collect();
    json(&list)
}

#[pymodule]
fn _opendoc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NativeDocument>()?;
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_function(wrap_pyfunction!(build_docx, m)?)?;
    m.add_function(wrap_pyfunction!(formats, m)?)?;
    m.add("NativeError", m.py().get_type::<NativeError>())?;
    m.add("VERSION", opendoc::VERSION)?;
    Ok(())
}
