//! Scores a reflowed PDF's [`Summary`] against its source DOCX's, per family,
//! each in `0..=1` (1 is the source exactly). O(paragraphs² x text) for the
//! alignment, which is the cost of a ground-truth run, not of an import.

use serde_json::{Value, json};

use crate::model::{Summary, Tab};

/// Levenshtein distance over characters. O(a x b) time, O(b) memory.
#[must_use]
pub fn levenshtein(a: &[char], b: &[char]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous + usize::from(ca != cb);
            previous = row[j + 1];
            row[j + 1] = substitute.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// `1 - distance / longer length`; 1 for two empty strings.
#[must_use]
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let longer = a.len().max(b.len());
    if longer == 0 {
        return 1.0;
    }
    1.0 - levenshtein(&a, &b) as f64 / longer as f64
}

/// Pairs (source, output) of paragraphs, aligned in order by text
/// (Needleman-Wunsch, a pair scoring its similarity, a gap nothing).
fn align(src: &[String], out: &[String]) -> Vec<(usize, usize, f64)> {
    let (n, m) = (src.len(), out.len());
    let mut sim = vec![vec![0.0f64; m]; n];
    for (i, a) in src.iter().enumerate() {
        for (j, b) in out.iter().enumerate() {
            // Cheap reject: lengths far apart cannot be similar.
            let (la, lb) = (a.chars().count(), b.chars().count());
            if la.max(lb) > 3 * la.min(lb).max(1) {
                continue;
            }
            sim[i][j] = similarity(a, b);
        }
    }
    let mut best = vec![vec![0.0f64; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            let pair = if sim[i - 1][j - 1] >= 0.5 {
                best[i - 1][j - 1] + sim[i - 1][j - 1]
            } else {
                f64::NEG_INFINITY
            };
            best[i][j] = pair.max(best[i - 1][j]).max(best[i][j - 1]);
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let s = sim[i - 1][j - 1];
        if s >= 0.5 && (best[i][j] - (best[i - 1][j - 1] + s)).abs() < 1e-9 {
            pairs.push((i - 1, j - 1, s));
            i -= 1;
            j -= 1;
        } else if (best[i][j] - best[i - 1][j]).abs() < 1e-9 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

fn ratio(a: usize, b: usize) -> f64 {
    if a.max(b) == 0 {
        1.0
    } else {
        a.min(b) as f64 / a.max(b) as f64
    }
}

fn f1(tp: usize, fp: usize, fn_: usize) -> f64 {
    if tp + fp + fn_ == 0 {
        return 1.0;
    }
    2.0 * tp as f64 / (2 * tp + fp + fn_) as f64
}

fn table_score(src: &Tab, out: &Tab) -> f64 {
    let mut cells = 0usize;
    let mut found = 0.0;
    for (r, row) in src.cells.iter().enumerate() {
        for (c, text) in row.iter().enumerate() {
            cells += 1;
            if let Some(o) = out.cells.get(r).and_then(|row| row.get(c)) {
                found += similarity(text, o);
            }
        }
    }
    let shape = ratio(src.cells.len(), out.cells.len());
    if cells == 0 {
        return shape;
    }
    0.5 * shape + 0.5 * found / cells as f64
}

/// Every family's score and the evidence behind it, as JSON.
#[must_use]
pub fn score(src: &Summary, out: &Summary) -> Value {
    let src_texts: Vec<String> = src.paras.iter().map(|p| p.text.clone()).collect();
    let out_texts: Vec<String> = out.paras.iter().map(|p| p.text.clone()).collect();
    let pairs = align(&src_texts, &out_texts);
    let close = pairs.iter().filter(|p| p.2 >= 0.9).count();
    let clip = |t: &str| t.chars().take(60).collect::<String>();
    let unmatched_source: Vec<String> = (0..src.paras.len())
        .filter(|i| !pairs.iter().any(|p| p.0 == *i && p.2 >= 0.9))
        .map(|i| clip(&src.paras[i].text))
        .collect();
    let unmatched_output: Vec<String> = (0..out.paras.len())
        .filter(|j| !pairs.iter().any(|p| p.1 == *j && p.2 >= 0.9))
        .map(|j| clip(&out.paras[j].text))
        .collect();
    let all = |s: &Summary| {
        let mut text = s
            .paras
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for t in &s.tables {
            for row in &t.cells {
                text.push(' ');
                text.push_str(&row.join(" "));
            }
        }
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let text = similarity(&all(src), &all(out));
    // Headings and lists over the aligned pairs.
    let (mut htp, mut hfp, mut hfn, mut hlevel) = (0, 0, 0, 0);
    let (mut ltp, mut lfp, mut lfn, mut llevel) = (0, 0, 0, 0);
    let (mut bold, mut italic, mut size, mut formatted) = (0, 0, 0, 0);
    let mut heading_misses = Vec::new();
    // Layout over the close pairs: alignment, line pitch (within 12%), and
    // the space down to the next paragraph (after plus the next one's
    // before, as Word adds them; within 3 pt or a quarter of it).
    let (mut aligned, mut pitched, mut spaced, mut laid) = (0, 0, 0, 0);
    // Strict spacing (pitch within 3%, the space below within 1.5 pt),
    // indents (start and first line within 2 pt, outside lists) and exact
    // sizes (the same half point).
    let (mut tight_pitch, mut tight_space) = (0, 0);
    let (mut indented, mut indent_ok, mut exact_size) = (0, 0, 0);
    let mut size_misses = Vec::new();
    let mut indent_misses = Vec::new();
    let mut layout_misses = Vec::new();
    let gap =
        |s: &Summary, k: usize| s.paras[k].after + s.paras.get(k + 1).map_or(0.0, |n| n.before);
    for &(i, j, s) in &pairs {
        if s >= 0.9 {
            let (a, b) = (&src.paras[i], &out.paras[j]);
            laid += 1;
            let align = a.align == b.align;
            let pitch = (a.pitch - b.pitch).abs() <= 0.12 * a.pitch.max(1.0);
            let (ga, gb) = (gap(src, i), gap(out, j));
            let space = (ga - gb).abs() <= (0.25 * ga).max(3.0);
            aligned += usize::from(align);
            pitched += usize::from(pitch);
            spaced += usize::from(space);
            tight_pitch += usize::from((a.pitch - b.pitch).abs() <= 0.03 * a.pitch.max(1.0));
            tight_space += usize::from((ga - gb).abs() <= 1.5);
            exact_size += usize::from(a.size == b.size);
            if a.size != b.size && size_misses.len() < 12 {
                size_misses.push(json!({
                    "text": a.text.chars().take(30).collect::<String>(),
                    "size": [f64::from(a.size) / 2.0, f64::from(b.size) / 2.0],
                }));
            }
            if a.list.is_none() && b.list.is_none() && a.align != 1 {
                indented += 1;
                let ok = (a.indent.0 - b.indent.0).abs() <= 2.0
                    && (a.indent.1 - b.indent.1).abs() <= 2.0;
                indent_ok += usize::from(ok);
                if !ok && indent_misses.len() < 12 {
                    indent_misses.push(json!({
                        "text": a.text.chars().take(30).collect::<String>(),
                        "indent": [[a.indent.0, a.indent.1], [b.indent.0, b.indent.1]],
                    }));
                }
            }
            if !(align && pitch && space) && layout_misses.len() < 12 {
                layout_misses.push(json!({
                    "text": a.text.chars().take(30).collect::<String>(),
                    "align": [a.align, b.align],
                    "pitch": [(a.pitch * 10.0).round() / 10.0, (b.pitch * 10.0).round() / 10.0],
                    "space": [(ga * 10.0).round() / 10.0, (gb * 10.0).round() / 10.0],
                }));
            }
        }
    }
    for &(i, j, s) in &pairs {
        let (a, b) = (&src.paras[i], &out.paras[j]);
        match (a.heading, b.heading) {
            (Some(x), Some(y)) => {
                htp += 1;
                hlevel += usize::from(x == y);
            }
            (Some(_), None) => {
                hfn += 1;
                heading_misses.push(a.text.chars().take(40).collect::<String>());
            }
            (None, Some(_)) => hfp += 1,
            (None, None) => {}
        }
        match (a.list, b.list) {
            (Some(x), Some(y)) => {
                ltp += 1;
                llevel += usize::from(x == y);
            }
            (Some(_), None) => lfn += 1,
            (None, Some(_)) => lfp += 1,
            (None, None) => {}
        }
        if s >= 0.9 {
            formatted += 1;
            bold += usize::from(a.bold == b.bold);
            italic += usize::from(a.italic == b.italic);
            size += usize::from(a.size.abs_diff(b.size) <= 1);
        }
    }
    let src_headings = src.paras.iter().filter(|p| p.heading.is_some()).count();
    let src_lists = src.paras.iter().filter(|p| p.list.is_some()).count();
    let share = |n: usize, d: usize| if d == 0 { 1.0 } else { n as f64 / d as f64 };
    // Tables are aligned in order by their text, as paragraphs are, so a
    // source table the reflow kept as paragraphs (a one-cell callout box)
    // costs that table, not every table after it.
    let table_text = |t: &Tab| {
        t.cells
            .iter()
            .map(|row| row.join(" "))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let table_pairs = align(
        &src.tables.iter().map(table_text).collect::<Vec<_>>(),
        &out.tables.iter().map(table_text).collect::<Vec<_>>(),
    );
    let tables = if src.tables.is_empty() && out.tables.is_empty() {
        1.0
    } else {
        let total: f64 = table_pairs
            .iter()
            .map(|&(i, j, _)| table_score(&src.tables[i], &out.tables[j]))
            .sum();
        total / src.tables.len().max(out.tables.len()) as f64
    };
    // The same, over the tables of more than one cell (a one-cell table is a
    // shaded box, a paragraph in every flowing reading of a PDF).
    let multi = |s: &Summary| -> Vec<Tab> {
        s.tables
            .iter()
            .filter(|t| t.cells.len() * t.cells.iter().map(Vec::len).max().unwrap_or(0) > 1)
            .cloned()
            .collect()
    };
    let (src_multi, out_multi) = (multi(src), multi(out));
    let multi_pairs = align(
        &src_multi.iter().map(table_text).collect::<Vec<_>>(),
        &out_multi.iter().map(table_text).collect::<Vec<_>>(),
    );
    let grid_tables = if src_multi.is_empty() && out_multi.is_empty() {
        1.0
    } else {
        multi_pairs
            .iter()
            .map(|&(i, j, _)| table_score(&src_multi[i], &out_multi[j]))
            .sum::<f64>()
            / src_multi.len().max(out_multi.len()) as f64
    };
    // Table geometry over the aligned multi-cell tables of the same shape:
    // each column's share of the table's width (within 2 points of the
    // source's share of its own) and each cell's alignment.
    let (mut columns, mut columns_ok, mut cells_aligned, mut cells_total) = (0, 0, 0, 0);
    let mut table_misses = Vec::new();
    for &(i, j, _) in &multi_pairs {
        let (a, b) = (&src_multi[i], &out_multi[j]);
        let (sa, sb): (f64, f64) = (a.widths.iter().sum(), b.widths.iter().sum());
        if a.widths.len() == b.widths.len() && sa > 0.0 && sb > 0.0 {
            for (wa, wb) in a.widths.iter().zip(&b.widths) {
                columns += 1;
                columns_ok += usize::from((wa / sa - wb / sb).abs() * sa <= 2.0);
            }
        } else {
            columns += a.widths.len().max(1);
        }
        for (r, row) in a.aligns.iter().enumerate() {
            for (c, &al) in row.iter().enumerate() {
                if a.cells[r][c].is_empty() {
                    continue;
                }
                cells_total += 1;
                let got = b.aligns.get(r).and_then(|row| row.get(c)).copied();
                // Both (justified) reads as start in a one-line cell.
                let same = got.is_some_and(|g| g == al || (g.max(al) == 3 && g.min(al) == 0));
                cells_aligned += usize::from(same);
                if !same && table_misses.len() < 12 {
                    table_misses.push(json!({
                        "cell": a.cells[r][c].chars().take(20).collect::<String>(),
                        "align": [al, got],
                    }));
                }
            }
        }
    }
    let table_layout = if columns + cells_total == 0 {
        1.0
    } else {
        0.5 * share(columns_ok, columns) + 0.5 * share(cells_aligned, cells_total)
    };
    // Pictures in order: the extent within 1% (or 1 point).
    let (mut pictures_exact, pictures_compared) =
        (0, src.picture_sizes.len().max(out.picture_sizes.len()));
    for (a, b) in src.picture_sizes.iter().zip(&out.picture_sizes) {
        let close = |x: f64, y: f64| (x - y).abs() <= (0.01 * x).max(1.0);
        pictures_exact += usize::from(close(a.0, b.0) && close(a.1, b.1));
    }
    let sizes = if laid + pictures_compared == 0 {
        1.0
    } else {
        (exact_size + pictures_exact) as f64 / (laid + pictures_compared) as f64
    };
    let watermark = match (&src.watermark, &out.watermark) {
        (None, None) => 1.0,
        (Some((ta, da)), Some((tb, db))) => {
            0.5 + 0.25 * f64::from(u8::from(similarity(ta, tb) >= 0.9))
                + 0.25 * f64::from(u8::from(da == db))
        }
        _ => 0.0,
    };
    let mut special = 0usize;
    let mut special_total = 0usize;
    for (c, n) in &src.non_ascii {
        special_total += n;
        special += (*n).min(out.non_ascii.get(c).copied().unwrap_or(0));
    }
    let missing_special: Vec<String> = src
        .non_ascii
        .iter()
        .filter(|(c, n)| out.non_ascii.get(c).copied().unwrap_or(0) < **n)
        .map(|(c, n)| {
            format!(
                "U+{:04X}:{}/{}",
                u32::from(*c),
                out.non_ascii.get(c).copied().unwrap_or(0),
                n
            )
        })
        .collect();
    let margin_diff = {
        let (a, b) = (src.margins, out.margins);
        ((a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs() + (a.3 - b.3).abs()) as f64 / 4.0
    };
    let page = if (src.page.0 - out.page.0).abs() <= 40 && (src.page.1 - out.page.1).abs() <= 40 {
        1.0 - (margin_diff / 1440.0).min(1.0) * 0.5
    } else {
        0.0
    };
    let header = 0.4 * similarity(&src.header, &out.header)
        + 0.4 * similarity(&src.footer, &out.footer)
        + 0.2 * f64::from(u8::from((src.page_fields > 0) == (out.page_fields > 0)));
    json!({
        "text": text,
        "paragraphs": share(close, src.paras.len().max(out.paras.len())),
        "headings": f1(htp, hfp, hfn) * if htp == 0 { 1.0 } else { 0.5 + 0.5 * share(hlevel, htp) },
        "lists": f1(ltp, lfp, lfn) * if ltp == 0 { 1.0 } else { 0.5 + 0.5 * share(llevel, ltp) },
        "tables": tables,
        "grid_tables": grid_tables,
        "pictures": ratio(src.pictures, out.pictures),
        "header_footer": header,
        "notes": ratio(src.notes, out.notes),
        "columns": f64::from(u8::from(src.columns == out.columns)),
        "page": page,
        "formatting": if formatted == 0 { 0.0 } else { (bold + italic + size) as f64 / (3 * formatted) as f64 },
        "special": share(special, special_total),
        "frames": f64::from(u8::from(out.text_boxes == 0)),
        "layout": if laid == 0 { 0.0 } else { (aligned + pitched + spaced) as f64 / (3 * laid) as f64 },
        "spacing": if laid == 0 { 0.0 } else { (tight_pitch + tight_space) as f64 / (2 * laid) as f64 },
        "indents": share(indent_ok, indented),
        "sizes": sizes,
        "table_layout": table_layout,
        "watermark": watermark,
        "evidence": {
            "paragraphs": [src.paras.len(), out.paras.len(), pairs.len(), close],
            "unmatched_source": unmatched_source,
            "unmatched_output": unmatched_output,
            "headings": { "source": src_headings, "tp": htp, "fp": hfp, "fn": hfn, "level": hlevel, "missed": heading_misses },
            "lists": { "source": src_lists, "tp": ltp, "fp": lfp, "fn": lfn, "level": llevel },
            "tables": [src.tables.iter().map(|t| [t.cells.len(), t.cells.iter().map(Vec::len).max().unwrap_or(0), t.merges]).collect::<Vec<_>>(),
                       out.tables.iter().map(|t| [t.cells.len(), t.cells.iter().map(Vec::len).max().unwrap_or(0), t.merges]).collect::<Vec<_>>()],
            "pictures": [src.pictures, out.pictures],
            "header": [src.header, out.header],
            "footer": [src.footer, out.footer],
            "page_fields": [src.page_fields, out.page_fields],
            "notes": [src.notes, out.notes],
            "columns": [src.columns, out.columns],
            "page": [[src.page.0, src.page.1], [out.page.0, out.page.1]],
            "margins": [[src.margins.0, src.margins.1, src.margins.2, src.margins.3], [out.margins.0, out.margins.1, out.margins.2, out.margins.3]],
            "formatting": [formatted, bold, italic, size],
            "layout": [laid, aligned, pitched, spaced],
            "layout_misses": layout_misses,
            "spacing": [laid, tight_pitch, tight_space],
            "indents": [indented, indent_ok],
            "indent_misses": indent_misses,
            "sizes": [laid, exact_size, pictures_compared, pictures_exact],
            "size_misses": size_misses,
            "table_layout": [columns, columns_ok, cells_total, cells_aligned],
            "table_misses": table_misses,
            "watermark": [src.watermark.clone(), out.watermark.clone()],
            "bad_chars": [out.bad_chars.0, out.bad_chars.1, out.bad_chars.2],
            "missing_special": missing_special,
            "text_boxes": out.text_boxes,
            "groups": out.groups,
        }
    })
}
