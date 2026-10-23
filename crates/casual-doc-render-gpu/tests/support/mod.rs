//! The tolerance both parity tests hold the second backend to, and the
//! comparison that measures it. One definition, so the sheet and the documents
//! cannot drift apart (`docs/179` §6).
//!
//! Three numbers per page, against the CPU reference:
//!
//! * **mean channel difference ≤ [`MAX_MEAN`]** (0–255, over every channel of
//!   every pixel). Two anti-aliasers disagree by a few levels along every edge;
//!   a page of dense text sits near 2. Text one pixel off, a missing fill or a
//!   wrong colour moves it far more.
//! * **disagreeing pixels ≤ [`MAX_OVER_SHARE`] of the page**, where a pixel
//!   disagrees when its worst channel differs by more than [`PIXEL_THRESHOLD`]:
//!   ink on one backend and paper on the other. Edge disagreement is scattered
//!   and small; a missing border or underline is not.
//! * **disagreeing pixels in the worst [`TILE`]-pixel square ≤
//!   [`MAX_TILE_OVER`]**. The page-wide share can hide one missing word on a
//!   full page; the worst tile cannot, because legitimate disagreement is
//!   scattered and a missing object is concentrated.
//!
//! Not tuned per fixture. A page that needs more is a defect in the backend,
//! not a reason to loosen the gate (`SKILL` §5).

#![allow(dead_code)] // each test file uses part of it

/// Largest mean channel difference a page may show.
pub const MAX_MEAN: f64 = 3.0;
/// A pixel disagrees when its worst channel differs by more than this.
pub const PIXEL_THRESHOLD: u8 = 64;
/// Largest share of disagreeing pixels on a page: 0.1 %.
pub const MAX_OVER_SHARE: f64 = 0.001;
/// The side of the square the concentration check counts in, device pixels.
pub const TILE: usize = 32;
/// Largest number of disagreeing pixels in any one tile.
pub const MAX_TILE_OVER: usize = 48;

/// How far a candidate page is from the reference.
#[derive(Clone, Copy, Debug)]
pub struct Difference {
    /// Mean channel difference.
    pub mean: f64,
    /// Share of disagreeing pixels.
    pub over_share: f64,
    /// Disagreeing pixels in the worst tile.
    pub worst_tile: usize,
}

impl Difference {
    /// Whether this page is within the tolerance.
    pub fn within(&self) -> bool {
        self.mean <= MAX_MEAN
            && self.over_share <= MAX_OVER_SHARE
            && self.worst_tile <= MAX_TILE_OVER
    }
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mean {:.3} (max {MAX_MEAN}), disagreeing {:.4}% (max {:.4}%), worst tile {} (max {MAX_TILE_OVER})",
            self.mean,
            self.over_share * 100.0,
            MAX_OVER_SHARE * 100.0,
            self.worst_tile,
        )
    }
}

/// Compares two premultiplied-RGBA pages of `width` pixels per row.
pub fn difference(reference: &[u8], candidate: &[u8], width: usize) -> Difference {
    assert_eq!(reference.len(), candidate.len(), "the same page size");
    let height = reference.len() / 4 / width.max(1);
    let tiles_x = width.div_ceil(TILE);
    let mut tiles = vec![0usize; tiles_x * height.div_ceil(TILE)];
    let mut sum = 0u64;
    let mut over = 0usize;
    for (index, (a, b)) in reference
        .chunks_exact(4)
        .zip(candidate.chunks_exact(4))
        .enumerate()
    {
        let mut worst = 0u8;
        for channel in 0..4 {
            let d = a[channel].abs_diff(b[channel]);
            sum += u64::from(d);
            worst = worst.max(d);
        }
        if worst > PIXEL_THRESHOLD {
            over += 1;
            let (x, y) = (index % width, index / width);
            tiles[(y / TILE) * tiles_x + x / TILE] += 1;
        }
    }
    let pixels = (reference.len() / 4).max(1) as f64;
    Difference {
        mean: sum as f64 / (pixels * 4.0),
        over_share: over as f64 / pixels,
        worst_tile: tiles.into_iter().max().unwrap_or(0),
    }
}

/// With `OPENDOC_PARITY_DUMP=<dir>`, writes the reference, the candidate and a
/// mask of the disagreeing pixels as PPM, for a human to look at.
pub fn dump(label: &str, w: u32, h: u32, reference: &[u8], candidate: &[u8]) {
    let Some(dir) = std::env::var_os("OPENDOC_PARITY_DUMP") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let label = label.replace([' ', '/', '@'], "_");
    let write = |name: &str, pixel: &dyn Fn(usize) -> [u8; 3]| {
        let mut out = format!("P6 {w} {h} 255\n").into_bytes();
        for i in 0..(w * h) as usize {
            out.extend_from_slice(&pixel(i));
        }
        std::fs::write(dir.join(format!("{label}-{name}.ppm")), out).expect("dump");
    };
    let rgb = |data: &[u8], i: usize| [data[i * 4], data[i * 4 + 1], data[i * 4 + 2]];
    write("reference", &|i| rgb(reference, i));
    write("candidate", &|i| rgb(candidate, i));
    write("over", &|i| {
        let worst = (0..4)
            .map(|c| reference[i * 4 + c].abs_diff(candidate[i * 4 + c]))
            .max()
            .unwrap_or(0);
        if worst > PIXEL_THRESHOLD {
            [255, 0, 0]
        } else {
            [255, 255, 255]
        }
    });
}
