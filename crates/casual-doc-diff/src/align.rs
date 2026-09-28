//! Sequence alignment over hash keys.
//!
//! # The pattern, named before it was written
//!
//! Three established algorithms, composed, and one deliberately rejected:
//!
//! 1. **Common prefix/suffix trim.** Linear, and on a real version-to-version
//!    edit it removes almost everything.
//! 2. **Patience anchoring** (Bram Cohen; `git diff --patience`). Keys that occur
//!    **exactly once on each side** are unambiguous anchors; the longest
//!    increasing subsequence of their pairings splits the problem into
//!    independent small pieces. This is what keeps the edit distance inside each
//!    piece small, and small `D` is the whole reason the next step is affordable.
//! 3. **Myers' greedy O(ND) diff** (Myers 1986) on each remaining piece. `D` is
//!    the edit distance, not the length, so an unchanged document costs one pass.
//!
//! **Rejected: general tree edit distance** (Zhang–Shasha and its descendants).
//! It is O(n²) at best with the tree-depth factors on top, it needs a
//! relabel/reparent edit script a reader has no vocabulary for, and it solves a
//! problem this model does not have: a document body is an *ordered* forest with
//! typed containers, so aligning sibling lists and recursing into matched
//! containers is both exact and near-linear. The hierarchy is handled by
//! [`crate::projection`]'s subtree hashes, which let an unchanged container be
//! settled with one key comparison.
//!
//! # Complexity
//!
//! Trim is O(n). Anchoring is O(n log n) (the LIS binary search). Each leaf piece
//! is bounded by [`MAX_ALIGN_CELLS`] and [`MAX_ALIGN_DISTANCE`], and leaves are
//! disjoint, so the total is **O(n log n)** in the length of the two sequences,
//! with a hard ceiling on the work any single piece may cost. A piece that would
//! exceed either bound is reported as a wholesale delete-plus-insert and sets
//! [`Alignment::degraded`] — bounded work and an explicit finding, rather than a
//! long stall or a silent guess.

use std::collections::HashMap;

/// The largest `left.len() * right.len()` a piece may cost before alignment
/// degrades to delete-plus-insert.
///
/// 1,048,576 cells is the point where Myers' inner loop stops being
/// imperceptible. Reaching it means the two sides share keys but no *unique*
/// one — every landmark is duplicated — so an exact alignment is a choice
/// between equally good answers, and the honest one is to say so.
///
/// The much commoner degenerate case, two regions with **no** common key at all,
/// never reaches Myers: it is detected while counting keys and reported as a
/// wholesale replacement, which is not a degradation but the correct answer.
pub const MAX_ALIGN_CELLS: usize = 1 << 20;

/// The largest edit distance Myers will trace inside one piece.
///
/// The trace costs O(D²) memory, so this is the memory bound: 1,024 is ~1 M
/// `i64` at worst, about 8 MB, and it is only reached by a piece with no unique
/// common key and a thousand differences.
pub const MAX_ALIGN_DISTANCE: usize = 1024;

/// One alignment step, in sequence order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    /// The same key on both sides, at these two offsets.
    Equal(u32, u32),
    /// A left-only key at this offset.
    Delete(u32),
    /// A right-only key at this offset.
    Insert(u32),
}

/// The result of aligning two key sequences.
#[derive(Clone, Debug, Default)]
pub struct Alignment {
    /// The steps, in sequence order.
    pub steps: Vec<Step>,
    /// Key comparisons performed. The quantity the complexity guard measures.
    pub comparisons: u64,
    /// Whether any piece exceeded a bound and was reported wholesale.
    pub degraded: bool,
}

/// A pending piece of the alignment, or an anchor to emit.
#[derive(Clone, Copy, Debug)]
enum Task {
    Align { left: (u32, u32), right: (u32, u32) },
    Equal(u32, u32),
}

/// Aligns two key sequences.
///
/// `left` and `right` are the alignment keys — subtree hashes, from
/// [`crate::projection`] — offset by `left_base`/`right_base` so the emitted
/// [`Step`]s carry absolute indices into the projection.
///
/// **O(n log n)** with the per-piece ceilings described in the module docs.
#[must_use]
pub fn align(left: &[u128], right: &[u128], left_base: u32, right_base: u32) -> Alignment {
    let mut out = Alignment::default();
    let mut stack = vec![Task::Align {
        left: (0, u32::try_from(left.len()).unwrap_or(u32::MAX)),
        right: (0, u32::try_from(right.len()).unwrap_or(u32::MAX)),
    }];
    while let Some(task) = stack.pop() {
        match task {
            Task::Equal(li, ri) => out.steps.push(Step::Equal(left_base + li, right_base + ri)),
            Task::Align {
                left: (ls, le),
                right: (rs, re),
            } => {
                split(
                    left,
                    right,
                    (ls, le),
                    (rs, re),
                    left_base,
                    right_base,
                    &mut out,
                    &mut stack,
                );
            }
        }
    }
    out
}

/// Trims, anchors, and either queues sub-pieces or emits a leaf.
#[allow(clippy::too_many_arguments)]
fn split(
    left: &[u128],
    right: &[u128],
    (mut ls, mut le): (u32, u32),
    (mut rs, mut re): (u32, u32),
    left_base: u32,
    right_base: u32,
    out: &mut Alignment,
    stack: &mut Vec<Task>,
) {
    // Step 1: common prefix. Emitted immediately, so it needs no task.
    while ls < le && rs < re {
        out.comparisons += 1;
        if left[ls as usize] != right[rs as usize] {
            break;
        }
        out.steps.push(Step::Equal(left_base + ls, right_base + rs));
        ls += 1;
        rs += 1;
    }
    // Step 1b: common suffix. Held back and pushed as tasks so it lands after
    // whatever the middle produces.
    let mut suffix = Vec::new();
    while ls < le && rs < re {
        out.comparisons += 1;
        if left[le as usize - 1] != right[re as usize - 1] {
            break;
        }
        le -= 1;
        re -= 1;
        suffix.push(Task::Equal(le, re));
    }

    // Push the suffix first so it is popped last (the stack is LIFO).
    for task in &suffix {
        stack.push(*task);
    }

    if ls == le && rs == re {
        return;
    }
    if ls == le {
        out.steps
            .extend((rs..re).map(|index| Step::Insert(right_base + index)));
        return;
    }
    if rs == re {
        out.steps
            .extend((ls..le).map(|index| Step::Delete(left_base + index)));
        return;
    }

    // Step 2: patience anchors.
    let (anchors, common) = unique_anchors(
        &left[ls as usize..le as usize],
        &right[rs as usize..re as usize],
    );
    out.comparisons += u64::from(le - ls) + u64::from(re - rs);
    if common == 0 {
        // The two regions have nothing in common. Myers would spend O(n·(n+m))
        // proving it; the counting pass already did, in O(n). This is NOT a
        // degradation — every block is genuinely new or gone, and the second
        // pass in `job::resolve_run` still pairs them up where the pairing is
        // unambiguous.
        emit_wholesale(out, (ls, le), (rs, re), left_base, right_base);
        return;
    }
    if !anchors.is_empty() {
        // Sub-pieces and anchors, pushed in reverse so they pop in order.
        let mut tasks = Vec::with_capacity(anchors.len() * 2 + 1);
        let mut cursor = (ls, rs);
        for (li, ri) in &anchors {
            let (li, ri) = (ls + li, rs + ri);
            tasks.push(Task::Align {
                left: (cursor.0, li),
                right: (cursor.1, ri),
            });
            tasks.push(Task::Equal(li, ri));
            cursor = (li + 1, ri + 1);
        }
        tasks.push(Task::Align {
            left: (cursor.0, le),
            right: (cursor.1, re),
        });
        for task in tasks.into_iter().rev() {
            stack.push(task);
        }
        return;
    }

    // Step 3: Myers, or an explicit wholesale fallback.
    let cells = (le - ls) as usize * (re - rs) as usize;
    if cells > MAX_ALIGN_CELLS {
        out.degraded = true;
        emit_wholesale(out, (ls, le), (rs, re), left_base, right_base);
        return;
    }
    match myers(
        &left[ls as usize..le as usize],
        &right[rs as usize..re as usize],
        out,
    ) {
        Some(steps) => {
            for step in steps {
                out.steps.push(match step {
                    Step::Equal(li, ri) => Step::Equal(left_base + ls + li, right_base + rs + ri),
                    Step::Delete(li) => Step::Delete(left_base + ls + li),
                    Step::Insert(ri) => Step::Insert(right_base + rs + ri),
                });
            }
        }
        None => {
            out.degraded = true;
            emit_wholesale(out, (ls, le), (rs, re), left_base, right_base);
        }
    }
}

/// Reports a piece as every left key deleted and every right key inserted.
fn emit_wholesale(
    out: &mut Alignment,
    (ls, le): (u32, u32),
    (rs, re): (u32, u32),
    left_base: u32,
    right_base: u32,
) {
    for index in ls..le {
        out.steps.push(Step::Delete(left_base + index));
    }
    for index in rs..re {
        out.steps.push(Step::Insert(right_base + index));
    }
}

/// The longest increasing subsequence of the pairings of keys that occur exactly
/// once on each side.
///
/// Also returns how many distinct keys occur on **both** sides at all, which is
/// what tells a caller apart a region that needs Myers from one that shares no
/// landmark whatsoever.
///
/// This is patience diff's anchor selection. **O(n + k log k)**: one counting
/// pass, then a binary-search LIS over the `k` unique matches.
fn unique_anchors(left: &[u128], right: &[u128]) -> (Vec<(u32, u32)>, usize) {
    let mut counts: HashMap<u128, (u32, u32, u32, u32)> = HashMap::new();
    for (index, key) in left.iter().enumerate() {
        let entry = counts.entry(*key).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 = u32::try_from(index).unwrap_or(u32::MAX);
    }
    for (index, key) in right.iter().enumerate() {
        let entry = counts.entry(*key).or_insert((0, 0, 0, 0));
        entry.2 += 1;
        entry.3 = u32::try_from(index).unwrap_or(u32::MAX);
    }
    let common = counts
        .values()
        .filter(|(left_count, _, right_count, _)| *left_count > 0 && *right_count > 0)
        .count();
    let mut pairs: Vec<(u32, u32)> = counts
        .values()
        .filter(|(left_count, _, right_count, _)| *left_count == 1 && *right_count == 1)
        .map(|(_, left_index, _, right_index)| (*left_index, *right_index))
        .collect();
    if pairs.is_empty() {
        return (pairs, common);
    }
    pairs.sort_unstable();

    // Patience's LIS, by the card game it is named after: `tails[i]` is the
    // smallest right-index that can end an increasing run of length i + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; pairs.len()];
    for (position, (_, right_index)) in pairs.iter().enumerate() {
        let slot = tails.partition_point(|candidate| pairs[*candidate].1 < *right_index);
        previous[position] = slot.checked_sub(1).map(|index| tails[index]);
        if slot == tails.len() {
            tails.push(position);
        } else {
            tails[slot] = position;
        }
    }
    let mut chain = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(position) = cursor {
        chain.push(pairs[position]);
        cursor = previous[position];
    }
    chain.reverse();
    (chain, common)
}

/// Myers' greedy forward D-path search, with the trace kept so the edit script
/// can be recovered.
///
/// Returns `None` when the edit distance exceeds [`MAX_ALIGN_DISTANCE`], which is
/// the memory bound rather than a time bound: the trace is O(D²).
///
/// **O((n + m) · D)** time, **O(D²)** memory.
fn myers(left: &[u128], right: &[u128], out: &mut Alignment) -> Option<Vec<Step>> {
    let n = left.len();
    let m = right.len();
    let max = (n + m).min(MAX_ALIGN_DISTANCE);
    // `v[k + offset]` is the furthest x reached on diagonal k.
    let offset = n + m + 1;
    let mut v = vec![0i64; 2 * (n + m) + 3];
    let mut trace: Vec<Vec<i64>> = Vec::new();
    for d in 0..=max {
        // The state at the START of iteration `d`, which is what the backtrack
        // needs: it asks "which diagonal did the path arrive on before step d".
        trace.push(v.clone());
        let d_i = d as i64;
        let mut k = -d_i;
        while k <= d_i {
            let index = (k + offset as i64) as usize;
            let mut x = if k == -d_i || (k != d_i && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = x - k;
            while (x as usize) < n && (y as usize) < m {
                out.comparisons += 1;
                if left[x as usize] != right[y as usize] {
                    break;
                }
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x as usize >= n && y as usize >= m {
                return Some(backtrack(&trace, left, right, offset));
            }
            k += 2;
        }
    }
    None
}

/// Walks the recorded traces backwards into an in-order edit script.
///
/// **O(D + n + m)**.
fn backtrack(trace: &[Vec<i64>], left: &[u128], right: &[u128], offset: usize) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut x = left.len() as i64;
    let mut y = right.len() as i64;
    for d in (0..trace.len()).rev() {
        let v = &trace[d];
        let d_i = d as i64;
        let k = x - y;
        let index = (k + offset as i64) as usize;
        let previous_k = if k == -d_i || (k != d_i && v[index - 1] < v[index + 1]) {
            k + 1
        } else {
            k - 1
        };
        let previous_index = (previous_k + offset as i64) as usize;
        let previous_x = v[previous_index];
        let previous_y = previous_x - previous_k;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            steps.push(Step::Equal(x as u32, y as u32));
        }
        if d == 0 {
            break;
        }
        if x == previous_x {
            y -= 1;
            steps.push(Step::Insert(y as u32));
        } else {
            x -= 1;
            steps.push(Step::Delete(x as u32));
        }
    }
    steps.reverse();
    steps
}
