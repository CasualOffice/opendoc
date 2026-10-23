// SPDX-License-Identifier: Apache-2.0

//! Resource bounds (`docs/178` §4.3 rule 5, §5.3, §5.7). Defaults are the
//! proposed ones; a host may lower every one of them, never raise them past the
//! ceilings an untrusted file is held to.

/// Every bound the AI layer enforces.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Limits {
    /// Largest file `open` accepts, in bytes.
    pub max_file_bytes: usize,
    /// Default `max_tokens` for a text-bearing result.
    pub default_max_tokens: u32,
    /// Ceiling on any `max_tokens` a caller passes.
    pub max_tokens_ceiling: u32,
    /// Largest page of a paged list.
    pub max_page_items: usize,
    /// Most edits in one plan.
    pub max_plan_edits: usize,
    /// Most inserted text in one plan, in bytes.
    pub max_plan_text_bytes: usize,
    /// Default render DPI.
    pub default_dpi: f32,
    /// Ceiling on render DPI.
    pub max_dpi: f32,
    /// Ceiling on one rendered page's pixel count.
    pub max_page_pixels: u64,
    /// Wall-clock budget for one comparison, in milliseconds.
    pub compare_budget_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            default_max_tokens: 4_000,
            max_tokens_ceiling: 20_000,
            max_page_items: 200,
            max_plan_edits: 500,
            max_plan_text_bytes: 1024 * 1024,
            default_dpi: 110.0,
            max_dpi: 200.0,
            max_page_pixels: 40_000_000,
            compare_budget_ms: 20_000,
        }
    }
}

impl Limits {
    /// A caller's `max_tokens`, defaulted and held to the ceiling.
    #[must_use]
    pub fn tokens(&self, requested: Option<u32>) -> u32 {
        requested
            .unwrap_or(self.default_max_tokens)
            .clamp(1, self.max_tokens_ceiling)
    }

    /// A caller's page size, defaulted and held to the ceiling.
    #[must_use]
    pub fn page(&self, requested: Option<usize>, default: usize) -> usize {
        requested.unwrap_or(default).clamp(1, self.max_page_items)
    }
}
