//! Exact bounded tree edit distance whose TopDiff keyroot/top-node pairs are
//! filtered using SED-STRUCT structural annotations.
//!
//! This crate fuses two existing algorithms of the workspace:
//!   * **TopDiff** (`ted-distance-topdiff`) — Touzet KR-set bounded TED. It runs
//!     a per-keyroot-pair forest DP (`tree_dist`) over a set of "top-node pairs"
//!     (keyroot-ancestor pairs), and returns the exact TED when `<= k`, else
//!     `k + 1`.
//!   * **SED-STRUCT** (`ted-lb-sed-struct`) — a structural string-edit lower
//!     bound. Each node carries `sum`/`diff` annotations derived in a single
//!     postorder DFS.
//!
//! TopDiff collects its top-node pairs with a brute-force `O(n*k)` loop over the
//! k-strip guarded by `k_relevant`. Here we additionally require the SED-STRUCT
//! per-node structural constraint (`|sum_x - sum_y| <= k` and
//! `|diff_x - diff_y| <= k`) to hold for the underlying `(x, y)` before a
//! top-node pair is admitted. The structural annotations are computed in
//! preprocessing and looked up per candidate pair.
//!
//! ## Exactness
//! `tree_dist` for an outer keyroot pair reads inner subtree distances from the
//! `td` band matrix; those cells exist only if the inner keyroot pair was also
//! collected and processed earlier (pairs run inner-first). So the collected set
//! must be a **superset** of the pairs TopDiff's own `k_relevant` loop would
//! collect. The extra structural filter (`collect_pairs_struct`) is therefore an
//! empirical bet: it is correct iff it never drops a needed pair. The
//! always-correct `k_relevant`-only collection (`collect_pairs_full`) is kept as
//! both the differential-test oracle and a runtime fallback, and the
//! differential test asserts `collect_pairs_full ⊆ collect_pairs_struct` across
//! a broad random sweep. `USE_STRUCT_FILTER` toggles which collection drives the
//! shipped driver.

use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod, TraversalKind, TraversalSelection};
use tree_parsing::{LabelId, ParsedTree};

/// When true the driver collects top-node pairs with the extra SED-STRUCT
/// structural filter (`collect_pairs_struct`); when false it uses the
/// always-correct `k_relevant`-only collection (`collect_pairs_full`). The
/// differential test gates this: it must stay `false` if the superset assertion
/// ever fails.
const USE_STRUCT_FILTER: bool = true;

/// A traversal element annotated with SED-STRUCT structural metrics. Copied from
/// `ted_lb_sed_struct::TraversalCharacter`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TraversalCharacter {
    pub char: LabelId,
    pub sum: i32,
    pub diff: i32,
}

/// Combined per-tree index: TopDiff postorder arrays + SED-STRUCT annotations.
///
/// All postorder-indexed arrays mirror `ted_distance_topdiff::TopDiffIndex`
/// (same field names and invariants). `postl_struct` holds the postorder
/// SED-STRUCT `TraversalCharacter` for each node (used by the per-pair
/// structural filter), and `first_traversal`/`second_traversal` hold the two
/// selected traversals used by the structural lower-bound early prune.
#[derive(Debug, Clone)]
pub struct StructDiffIndex {
    pub tree_size: i32,
    pub postl_to_label_id: Vec<i32>,
    pub postl_to_size: Vec<i32>,
    pub postl_to_depth: Vec<i32>,
    pub postl_to_lch: Vec<i32>,
    pub postl_to_kr_ancestor: Vec<i32>,
    pub list_kr: Vec<i32>,
    /// Postorder-indexed structural annotations (for the per-pair filter).
    pub postl_struct: Vec<TraversalCharacter>,
    /// Selected traversal A (for the SED-STRUCT lower-bound early prune).
    pub first_traversal: Vec<TraversalCharacter>,
    /// Selected traversal B.
    pub second_traversal: Vec<TraversalCharacter>,
}

impl StructDiffIndex {
    pub fn get_size(&self) -> usize {
        self.tree_size as usize
    }

    pub fn from_tree(tree: &ParsedTree, selection: TraversalSelection) -> Self {
        preprocess_tree(tree, selection)
    }
}

#[derive(Default)]
struct TraversalBuffers {
    preorder: Vec<TraversalCharacter>,
    postorder: Vec<TraversalCharacter>,
    reversed_preorder: Vec<TraversalCharacter>,
    reversed_postorder: Vec<TraversalCharacter>,
}

fn pick_traversal(kind: TraversalKind, buffers: &TraversalBuffers) -> &Vec<TraversalCharacter> {
    match kind {
        TraversalKind::Preorder => &buffers.preorder,
        TraversalKind::Postorder => &buffers.postorder,
        TraversalKind::ReversedPreorder => &buffers.reversed_preorder,
        TraversalKind::ReversedPostorder => &buffers.reversed_postorder,
    }
}

/// Single postorder DFS that fills both the TopDiff arrays and the SED-STRUCT
/// traversal buffers. Merges `TopDiffIndex::recurse` and
/// `ted_lb_sed_struct::traverse_with_info` — both derive `postorder_id`,
/// `depth` and `subtree_size` from the same traversal.
fn preprocess_tree(tree: &ParsedTree, selection: TraversalSelection) -> StructDiffIndex {
    let tree_size = tree.count();

    let mut postl_to_label_id = vec![0i32; tree_size];
    let mut postl_to_size = vec![0i32; tree_size];
    let mut postl_to_depth = vec![0i32; tree_size];
    let mut postl_to_lch = vec![-1i32; tree_size];
    let mut list_kr: Vec<i32> = Vec::new();
    let mut buffers = TraversalBuffers::default();

    let mut next_postorder: i32 = 0;

    let root = tree.iter().next().and_then(|node| tree.get_node_id(node));
    if let Some(root) = root {
        recurse(
            root,
            tree,
            tree_size as i32,
            0, // root depth == 0
            &mut next_postorder,
            &mut postl_to_label_id,
            &mut postl_to_size,
            &mut postl_to_depth,
            &mut postl_to_lch,
            &mut list_kr,
            &mut buffers,
        );
        // Root is the last keyroot (TopDiff: list_kr_.push_back(start_postorder - 1)).
        list_kr.push(next_postorder - 1);
    }

    // fill_kr_ancestors: walk the leftmost-child chain from each keyroot.
    let mut postl_to_kr_ancestor = vec![0i32; tree_size];
    for &i in &list_kr {
        let mut l = i;
        while l >= 0 {
            postl_to_kr_ancestor[l as usize] = i;
            l = postl_to_lch[l as usize];
        }
    }

    buffers.reversed_preorder.reverse();
    buffers.reversed_postorder.reverse();

    let first_traversal = pick_traversal(selection.first, &buffers).clone();
    let second_traversal = pick_traversal(selection.second, &buffers).clone();
    let postl_struct = buffers.postorder.clone();

    StructDiffIndex {
        tree_size: tree_size as i32,
        postl_to_label_id,
        postl_to_size,
        postl_to_depth,
        postl_to_lch,
        postl_to_kr_ancestor,
        list_kr,
        postl_struct,
        first_traversal,
        second_traversal,
    }
}

#[allow(clippy::too_many_arguments)]
fn recurse(
    nid: NodeId,
    tree: &ParsedTree,
    tree_size: i32,
    depth: i32,
    next_postorder: &mut i32,
    postl_to_label_id: &mut [i32],
    postl_to_size: &mut [i32],
    postl_to_depth: &mut [i32],
    postl_to_lch: &mut [i32],
    list_kr: &mut Vec<i32>,
    buffers: &mut TraversalBuffers,
) -> i32 {
    let label = *tree.get(nid).unwrap().get();

    // SED-STRUCT: preorder + reversed-postorder elements are emitted on entry
    // (their structural fields are back-patched after the subtree is known).
    let pre_idx = buffers.preorder.len();
    buffers.preorder.push(TraversalCharacter {
        char: label,
        sum: 0,
        diff: 0,
    });
    buffers.reversed_postorder.push(TraversalCharacter {
        char: label,
        sum: 0,
        diff: 0,
    });

    let mut desc_sum = 0i32;
    let mut first_child_postorder: i32 = -1;
    let mut is_first = true;
    for cnid in nid.children(tree) {
        let child_size = recurse(
            cnid,
            tree,
            tree_size,
            depth + 1,
            next_postorder,
            postl_to_label_id,
            postl_to_size,
            postl_to_depth,
            postl_to_lch,
            list_kr,
            buffers,
        );
        desc_sum += child_size;
        let child_postorder = *next_postorder - 1;
        if is_first {
            first_child_postorder = child_postorder;
            is_first = false;
        } else {
            list_kr.push(child_postorder);
        }
    }

    let this_postorder = *next_postorder as usize;
    let subtree_size = desc_sum + 1;

    // TopDiff arrays.
    postl_to_label_id[this_postorder] = label;
    postl_to_size[this_postorder] = subtree_size;
    postl_to_depth[this_postorder] = depth;
    postl_to_lch[this_postorder] = first_child_postorder;

    // SED-STRUCT structural quantities (`*postorder_id` after increment ==
    // this_postorder + 1, matching traverse_with_info).
    let postorder_count = this_postorder as i32 + 1;
    let preceding = postorder_count - subtree_size;
    let following = tree_size - (postorder_count + depth);
    let descendant = subtree_size - 1;
    let ancestor = depth;

    let pre = &mut buffers.preorder[pre_idx];
    pre.sum = following + descendant;
    pre.diff = descendant - following;

    let rev_post = &mut buffers.reversed_postorder[pre_idx];
    rev_post.sum = preceding + ancestor;
    rev_post.diff = ancestor - preceding;

    buffers.postorder.push(TraversalCharacter {
        char: label,
        sum: following + ancestor,
        diff: following - ancestor,
    });
    buffers.reversed_preorder.push(TraversalCharacter {
        char: label,
        sum: preceding + descendant,
        diff: preceding - descendant,
    });

    *next_postorder += 1;
    subtree_size
}

// ===========================================================================
// TopDiff DP machinery (copied from ted-distance-topdiff, retyped to
// &StructDiffIndex; field names are identical so bodies are unchanged).
// ===========================================================================

/// A specialised matrix where only the elements on a diagonal band matter.
/// Port of the C++ `BandMatrix<double>`.
#[derive(Debug, Clone)]
pub struct BandMatrix {
    columns: usize,
    band_width: usize,
    data: Vec<f64>,
}

impl BandMatrix {
    pub fn new(rows: usize, band_width: usize, fill: f64) -> Self {
        let columns = 2 * band_width + 1;
        Self {
            columns,
            band_width,
            data: vec![fill; rows * columns],
        }
    }

    #[inline]
    fn translate(&self, row: usize, col: usize) -> usize {
        let translated = col as isize + self.band_width as isize - row as isize;
        row * self.columns + translated as usize
    }

    #[inline]
    pub fn set(&mut self, row: usize, col: usize, value: f64) {
        let idx = self.translate(row, col);
        self.data[idx] = value;
    }

    #[allow(dead_code)]
    #[inline]
    pub fn at(&mut self, row: usize, col: usize) -> &mut f64 {
        let idx = self.translate(row, col);
        &mut self.data[idx]
    }

    #[inline]
    pub fn read_at(&self, row: usize, col: usize) -> f64 {
        let idx = self.translate(row, col);
        self.data[idx]
    }
}

/// Remaining error budget for the subtree pair `(x, y)`. Port of `e_budget`.
pub fn e_budget(t1: &StructDiffIndex, t2: &StructDiffIndex, x: i32, y: i32, k: i32) -> i32 {
    let x_size = t1.postl_to_size[x as usize];
    let y_size = t2.postl_to_size[y as usize];
    let dx = t1.postl_to_depth[x as usize];
    let dy = t2.postl_to_depth[y as usize];
    let lower_bound = ((t1.tree_size - (x + 1) - dx) - (t2.tree_size - (y + 1) - dy)).abs()
        + (dx - dy).abs()
        + (((x + 1) - x_size) - ((y + 1) - y_size)).abs();
    k - lower_bound
}

/// Whether subtrees `T1_x` and `T2_y` are k-relevant. Port of `k_relevant`.
pub fn k_relevant(t1: &StructDiffIndex, t2: &StructDiffIndex, x: i32, y: i32, k: i32) -> bool {
    let x_size = t1.postl_to_size[x as usize];
    let y_size = t2.postl_to_size[y as usize];
    let dx = t1.postl_to_depth[x as usize];
    let dy = t2.postl_to_depth[y as usize];
    let lower_bound = ((t1.tree_size - (x + 1) - dx) - (t2.tree_size - (y + 1) - dy)).abs()
        + (dx - dy).abs()
        + (x_size - y_size).abs()
        + (((x + 1) - x_size) - ((y + 1) - y_size)).abs();
    lower_bound <= k
}

/// The extra SED-STRUCT structural filter for a candidate node pair `(x, y)`,
/// looked up from the precomputed postorder annotations. Loosest form of the
/// per-character constraint used in `bounded_string_edit_distance_with_structure`.
#[inline]
fn struct_pair_ok(t1: &StructDiffIndex, t2: &StructDiffIndex, x: i32, y: i32, k: i32) -> bool {
    let c1 = &t1.postl_struct[x as usize];
    let c2 = &t2.postl_struct[y as usize];
    (c1.sum - c2.sum).abs() <= k && (c1.diff - c2.diff).abs() <= k
}

#[inline]
fn cost_ren(a: i32, b: i32) -> f64 {
    if a == b {
        0.0
    } else {
        1.0
    }
}

const COST_DEL: f64 = 1.0;
const COST_INS: f64 = 1.0;

/// Holds the band matrices `td`/`fd` and a subproblem counter. Port of
/// `TopDiffState`.
pub struct TopDiffState {
    pub td: BandMatrix,
    pub fd: BandMatrix,
    pub subproblem_counter: u64,
}

impl TopDiffState {
    pub fn new(t1_size: i32, k: i32) -> Self {
        let inf = f64::INFINITY;
        let td = BandMatrix::new(t1_size as usize, k as usize, inf);
        let fd = BandMatrix::new((t1_size + 1) as usize, (k + 1) as usize, inf);
        Self {
            td,
            fd,
            subproblem_counter: 0,
        }
    }

    /// Verbatim port of `TEDAlgorithmTouzet::tree_dist`.
    #[allow(clippy::needless_range_loop)]
    pub fn tree_dist(
        &mut self,
        t1: &StructDiffIndex,
        t2: &StructDiffIndex,
        x: i32,
        y: i32,
        k: i32,
        e: i32,
    ) -> f64 {
        let inf = f64::INFINITY;
        let x_size = t1.postl_to_size[x as usize];
        let y_size = t2.postl_to_size[y as usize];

        let x_off = x - x_size;
        let y_off = y - y_size;

        self.fd.set(0, 0, 0.0);
        let mut j = 1;
        while j <= y_size.min(e) {
            let v = self.fd.read_at(0, (j - 1) as usize) + COST_INS;
            self.fd.set(0, j as usize, v);
            j += 1;
        }
        if e < y_size {
            self.fd.set(0, (e + 1) as usize, inf);
        }

        let mut i = 1;
        while i <= x_size.min(e) {
            let v = self.fd.read_at((i - 1) as usize, 0) + COST_DEL;
            self.fd.set(i as usize, 0, v);
            i += 1;
        }
        if e < x_size {
            self.fd.set((e + 1) as usize, 0, inf);
        }

        let mut candidate_result = inf;

        for i in 1..=x_size {
            if i - e > 1 {
                self.fd.set(i as usize, (i - e - 1) as usize, inf);
            }
            let i_forest = i - t1.postl_to_size[(i + x_off) as usize];
            let mut j = (1).max(i - e);
            while j <= (i + e).min(y_size) {
                self.subproblem_counter += 1;

                let j_forest = j - t2.postl_to_size[(j + y_off) as usize];

                candidate_result = inf;
                candidate_result =
                    candidate_result.min(self.fd.read_at((i - 1) as usize, j as usize) + COST_DEL);
                candidate_result =
                    candidate_result.min(self.fd.read_at(i as usize, (j - 1) as usize) + COST_INS);

                let mut fd_read;
                if i_forest != 0 || j_forest != 0 {
                    let mut td_read = inf;
                    if ((i + x_off) - (j + y_off)).abs() <= k {
                        td_read = self.td.read_at((i + x_off) as usize, (j + y_off) as usize);
                    }
                    fd_read = inf;
                    if (0).max(i_forest - e - 1) <= j_forest
                        && j_forest <= (i_forest + e + 1).min(y_size)
                    {
                        fd_read = self.fd.read_at(i_forest as usize, j_forest as usize);
                    }
                    candidate_result = candidate_result.min(fd_read + td_read);
                } else {
                    fd_read = self.fd.read_at((i - 1) as usize, (j - 1) as usize)
                        + cost_ren(
                            t1.postl_to_label_id[(i + x_off) as usize],
                            t2.postl_to_label_id[(j + y_off) as usize],
                        );
                    candidate_result = candidate_result.min(fd_read);
                    if candidate_result <= e as f64 && ((i + x_off) - (j + y_off)).abs() <= k {
                        self.td
                            .set((i + x_off) as usize, (j + y_off) as usize, candidate_result);
                    }
                }

                if candidate_result > e as f64 {
                    self.fd.set(i as usize, j as usize, inf);
                } else {
                    self.fd.set(i as usize, j as usize, candidate_result);
                }
                j += 1;
            }
            if i + e < y_size {
                self.fd.set(i as usize, (i + e + 1) as usize, inf);
            }
        }

        if candidate_result > e as f64 {
            return inf;
        }
        candidate_result
    }
}

// ===========================================================================
// Top-node pair collection.
// ===========================================================================

/// Strategy B — TopDiff's exact `k_relevant`-only collection (the oracle and
/// runtime fallback). Port of `ted_k` lines collecting `kr_vector`.
fn collect_pairs_full(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> Vec<(i32, i32)> {
    collect_pairs(t1, t2, k, false)
}

/// Strategy A — `k_relevant` plus the SED-STRUCT per-pair structural filter.
fn collect_pairs_struct(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> Vec<(i32, i32)> {
    collect_pairs(t1, t2, k, true)
}

fn collect_pairs(
    t1: &StructDiffIndex,
    t2: &StructDiffIndex,
    k: i32,
    use_struct_filter: bool,
) -> Vec<(i32, i32)> {
    use rustc_hash::FxHashMap;

    let t1_size = t1.tree_size;
    let t2_size = t2.tree_size;

    let mut kr_pair_to_index: FxHashMap<u64, usize> = FxHashMap::default();
    let mut kr_vector: Vec<(i32, i32)> = Vec::new();

    for x in (0..t1_size).rev() {
        let x_keyroot = t1.postl_to_kr_ancestor[x as usize];
        let mut y = (x + k).min(t2_size - 1);
        let y_low = (0).max(x - k);
        while y >= y_low {
            if k_relevant(t1, t2, x, y, k)
                && (!use_struct_filter || struct_pair_ok(t1, t2, x, y, k))
            {
                let key =
                    ((x_keyroot as u64) << 32) | (t2.postl_to_kr_ancestor[y as usize] as u64);
                match kr_pair_to_index.get(&key) {
                    None => {
                        kr_pair_to_index.insert(key, kr_vector.len());
                        kr_vector.push((x, y));
                    }
                    Some(&idx) => {
                        if y > kr_vector[idx].1 {
                            kr_vector[idx].1 = y;
                        }
                    }
                }
            }
            y -= 1;
        }
    }

    kr_vector
}

// ===========================================================================
// SED-STRUCT lower bound (for the early prune). Copied from ted-lb-sed-struct.
// ===========================================================================

fn sed_struct_lb(t1: &StructDiffIndex, t2: &StructDiffIndex, k: usize) -> usize {
    let (mut a, mut b) = (t1, t2);
    if a.get_size().abs_diff(b.get_size()) > k {
        return k + 1;
    }
    if a.first_traversal.len() > b.first_traversal.len() {
        (a, b) = (b, a);
    }
    let first_dist =
        bounded_string_edit_distance_with_structure(&a.first_traversal, &b.first_traversal, k);
    if first_dist > k {
        return first_dist;
    }
    let second_dist =
        bounded_string_edit_distance_with_structure(&a.second_traversal, &b.second_traversal, k);
    std::cmp::max(first_dist, second_dist)
}

/// Bounded string edit distance with structural constraints (Berghel & Roach).
/// Copied verbatim from `ted_lb_sed_struct`. Assumes `s2.len() >= s1.len()`.
pub fn bounded_string_edit_distance_with_structure(
    s1: &[TraversalCharacter],
    s2: &[TraversalCharacter],
    k: usize,
) -> usize {
    use std::cmp::{max, min};
    let s1len = s1.len() as i32;
    let s2len = s2.len() as i32;
    let size_diff = s2len - s1len;
    let threshold = min(s2len, k as i32);

    let zero_k: i32 = threshold + 1;
    let array_size = (2 * threshold + 3) as usize;

    let mut current_row = vec![(-1i32, true); array_size];
    let mut next_row = vec![(-1i32, true); array_size];
    let target_diagonal = size_diff + zero_k;
    let target_diagonal_idx = target_diagonal as usize;
    let end_max = target_diagonal << 1;

    for i in 1..=threshold + 1 {
        std::mem::swap(&mut next_row, &mut current_row);

        let original_start: i32 = if i <= zero_k {
            -i + 1
        } else {
            i - (zero_k << 1) + 1
        };

        let original_end: i32;
        if i <= target_diagonal {
            original_end = i;
            unsafe {
                *next_row.get_unchecked_mut((zero_k + i) as usize) = (-1, true);
            }
        } else {
            original_end = end_max - i;
        }

        let budget = k as i32 - (i - 1);
        let (min_valid_diag, max_valid_diag) = if budget <= 0 {
            (size_diff, size_diff)
        } else {
            (size_diff - budget, size_diff + budget)
        };

        let start = max(original_start, min_valid_diag);
        let end = min(original_end, max_valid_diag + 1);

        let mut current_cell: i32;
        let mut next_cell: i32;
        let mut previous_cell: i32;
        let mut next_allowed_substitution: bool;

        if i <= zero_k && start == original_start {
            current_cell = -1;
            next_cell = i - 2i32;
            next_allowed_substitution = true;
        } else {
            unsafe {
                let start_idx = (zero_k + start) as usize;
                current_cell = if start > original_start && start_idx > 0 {
                    current_row.get_unchecked(start_idx - 1).0
                } else {
                    -1
                };
                (next_cell, next_allowed_substitution) = *current_row.get_unchecked(start_idx);
            }
        }

        let mut diagonal_index: usize = (start + zero_k).try_into().unwrap();
        let mut max_row_number;
        let allowed_edits = i - 1;

        let mut can_substitute: bool;
        for diag_offset in start..end {
            previous_cell = current_cell;
            current_cell = next_cell;
            can_substitute = next_allowed_substitution;
            unsafe {
                (next_cell, next_allowed_substitution) =
                    *current_row.get_unchecked(diagonal_index + 1);
            }

            unsafe {
                max_row_number = max(
                    current_cell + (if can_substitute { 1 } else { 0 }),
                    max(previous_cell, next_cell + 1),
                );

                if !can_substitute && max_row_number == current_cell {
                    *next_row.get_unchecked_mut(diagonal_index) = (max_row_number, false);
                    diagonal_index += 1;
                    continue;
                }
            }
            unsafe {
                let k = k as i32;
                let mut struct_ok = false;
                while max_row_number < s1len && (max_row_number + diag_offset) < s2len {
                    let c1 = s1.get_unchecked(max_row_number as usize);
                    let c2 = s2.get_unchecked((max_row_number + diag_offset) as usize);

                    let char_eq = c1.char == c2.char;
                    struct_ok = (allowed_edits + (c1.sum - c2.sum).abs() <= k)
                        && (allowed_edits + (c1.diff - c2.diff).abs() <= k);

                    if !char_eq || !struct_ok {
                        break;
                    }
                    max_row_number += 1;
                }

                *next_row.get_unchecked_mut(diagonal_index) = (max_row_number, struct_ok);
            }

            diagonal_index += 1;
        }

        unsafe {
            if next_row.get_unchecked(target_diagonal_idx).0 >= s1len {
                return (i - 1) as usize;
            }
        }
    }

    usize::MAX
}

// ===========================================================================
// Driver.
// ===========================================================================

/// Exact bounded TED via TopDiff with SED-STRUCT-filtered top-node pairs.
/// Returns the exact TED when `<= k`, otherwise `k + 1`.
pub fn ted_k_struct_diff(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> i32 {
    let t1_size = t1.tree_size;
    let t2_size = t2.tree_size;

    if (t1_size - t2_size).abs() > k {
        return k + 1;
    }

    // Early prune via the SED-STRUCT structural lower bound.
    if sed_struct_lb(t1, t2, k as usize) > k as usize {
        return k + 1;
    }

    let mut state = TopDiffState::new(t1_size, k);

    let mut pairs = if USE_STRUCT_FILTER {
        collect_pairs_struct(t1, t2, k)
    } else {
        collect_pairs_full(t1, t2, k)
    };

    // Inner-first ordering: ascending (x, y) so every inner keyroot pair (smaller
    // postorder id) is processed before any outer pair that reads its td cell.
    pairs.sort_unstable();

    for &(x_l, y_l) in &pairs {
        let e_max = e_budget(t1, t2, x_l, y_l, k);
        let d = state.tree_dist(t1, t2, x_l, y_l, k, e_max);
        state.td.set(x_l as usize, y_l as usize, d);
    }

    let result = state
        .td
        .read_at((t1_size - 1) as usize, (t2_size - 1) as usize);
    if !result.is_finite() || result > k as f64 {
        return k + 1;
    }
    result as i32
}

// ===========================================================================
// Public API.
// ===========================================================================

#[derive(Default)]
pub struct StructDiffAlgorithm {
    traversal_selection: TraversalSelection,
}

impl StructDiffAlgorithm {
    pub fn new(first: TraversalKind, second: TraversalKind) -> Self {
        Self {
            traversal_selection: TraversalSelection { first, second },
        }
    }
}

impl LowerBoundMethod for StructDiffAlgorithm {
    const NAME: &'static str = "SED-STRUCT-DIFF";
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = StructDiffIndex;
    type IndexType = ();
    type IndexParams = ();

    fn preprocess(&self, data: &[ParsedTree]) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data
            .iter()
            .map(|t| preprocess_tree(t, self.traversal_selection))
            .collect::<Vec<_>>())
    }

    fn lower_bound(
        &self,
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        ted_k_struct_diff(query, data, threshold as i32).max(0) as usize
    }

    fn build_index(
        &self,
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        Err("Indexing not supported for SED-STRUCT-DIFF".to_string())
    }

    fn query_index(
        &self,
        _query: &Self::PreprocessedDataType,
        _index: &Self::IndexType,
        _threshold: usize,
    ) -> Vec<usize> {
        vec![]
    }
}

pub struct StructDiffFactory;

impl AlgorithmFactory for StructDiffFactory {
    type AlgorithmType = StructDiffAlgorithm;
    fn create_algorithm() -> Self::AlgorithmType {
        StructDiffAlgorithm::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_parsing::{parse_single, LabelDict};

    fn pt(s: &str, dict: &mut LabelDict) -> ParsedTree {
        parse_single(s.to_string(), dict)
    }

    fn build(s: &str) -> StructDiffIndex {
        let mut dict = LabelDict::default();
        StructDiffIndex::from_tree(&pt(s, &mut dict), TraversalSelection::default())
    }

    fn sorted(v: &[i32]) -> Vec<i32> {
        let mut v = v.to_vec();
        v.sort_unstable();
        v
    }

    // ---- index construction (ported from ted-distance-topdiff) ----

    #[test]
    fn from_tree_single_node() {
        let idx = build("{a}");
        assert_eq!(idx.tree_size, 1);
        assert_eq!(idx.postl_to_size, vec![1]);
        assert_eq!(idx.postl_to_depth, vec![0]);
        assert_eq!(idx.postl_to_lch, vec![-1]);
        assert_eq!(idx.list_kr, vec![0]);
        assert_eq!(idx.postl_to_kr_ancestor, vec![0]);
    }

    #[test]
    fn from_tree_two_leaves() {
        let idx = build("{a{b}{c}}");
        assert_eq!(idx.tree_size, 3);
        assert_eq!(idx.postl_to_size, vec![1, 1, 3]);
        assert_eq!(idx.postl_to_depth, vec![1, 1, 0]);
        assert_eq!(idx.postl_to_lch, vec![-1, -1, 0]);
        assert_eq!(sorted(&idx.list_kr), vec![1, 2]);
        assert_eq!(idx.postl_to_kr_ancestor, vec![2, 1, 2]);
    }

    #[test]
    fn from_tree_nested() {
        let idx = build("{a{b{d}}{c}}");
        assert_eq!(idx.tree_size, 4);
        assert_eq!(idx.postl_to_size, vec![1, 2, 1, 4]);
        assert_eq!(idx.postl_to_depth, vec![2, 1, 1, 0]);
        assert_eq!(idx.postl_to_lch, vec![-1, 0, -1, 1]);
        assert_eq!(sorted(&idx.list_kr), vec![2, 3]);
        assert_eq!(idx.postl_to_kr_ancestor, vec![3, 3, 2, 3]);
    }

    #[test]
    fn postorder_struct_matches_sed_struct() {
        // {a{a{b{a{a}}}}} preorder sum/diff per ted-lb-sed-struct's own test.
        // Here we check the postorder annotations are consistent: leaf has the
        // smallest following+ancestor combination, root has following 0.
        let idx = build("{a{b}{c}}");
        // postorder ids: b=0, c=1, a=2. depth b=c=1, a=0. tree_size=3.
        // b: following = 3-(1+1)=1, ancestor=1 -> sum=2, diff=0
        // c: following = 3-(2+1)=0, ancestor=1 -> sum=1, diff=-1
        // a: following = 3-(3+0)=0, ancestor=0 -> sum=0, diff=0
        assert_eq!(idx.postl_struct[0], TraversalCharacter { char: idx.postl_struct[0].char, sum: 2, diff: 0 });
        assert_eq!(idx.postl_struct[1].sum, 1);
        assert_eq!(idx.postl_struct[1].diff, -1);
        assert_eq!(idx.postl_struct[2].sum, 0);
        assert_eq!(idx.postl_struct[2].diff, 0);
    }

    // ---- DP machinery (ported from ted-distance-topdiff) ----

    const INF: f64 = f64::INFINITY;

    #[test]
    fn band_matrix_translate_and_fill() {
        let mut m = BandMatrix::new(5, 4, INF);
        assert_eq!(m.read_at(2, 3), INF);
        m.set(2, 3, 7.0);
        assert_eq!(m.read_at(2, 3), 7.0);
        assert_eq!(*m.at(2, 3), 7.0);

        let mut z = BandMatrix::new(1, 0, INF);
        assert_eq!(z.read_at(0, 0), INF);
        z.set(0, 0, 3.5);
        assert_eq!(z.read_at(0, 0), 3.5);
    }

    fn tree_dist_roots(s1: &str, s2: &str, k: i32, e: i32) -> f64 {
        let mut dict = LabelDict::default();
        let sel = TraversalSelection::default();
        let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
        let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
        let mut state = TopDiffState::new(t1.tree_size, k);
        let x = t1.tree_size - 1;
        let y = t2.tree_size - 1;
        state.tree_dist(&t1, &t2, x, y, k, e)
    }

    #[test]
    fn tree_dist_basic() {
        assert_eq!(tree_dist_roots("{a}", "{a}", 4, 4), 0.0);
        assert_eq!(tree_dist_roots("{a}", "{b}", 4, 4), 1.0);
        assert_eq!(tree_dist_roots("{a{b}}", "{a}", 4, 4), 1.0);
    }

    #[test]
    fn e_budget_and_k_relevant() {
        let t1 = build("{a{b}{c}}");
        assert_eq!(e_budget(&t1, &t1, 2, 2, 5), 5);
        assert!(k_relevant(&t1, &t1, 2, 2, 5));

        let mut dict = LabelDict::default();
        let sel = TraversalSelection::default();
        let a = StructDiffIndex::from_tree(&pt("{a{b}{c}}", &mut dict), sel);
        let b = StructDiffIndex::from_tree(&pt("{a{b{d}}{c}}", &mut dict), sel);
        assert_eq!(e_budget(&a, &b, 0, 3, 0), -2);
        assert!(!k_relevant(&a, &b, 0, 3, 0));
    }

    // ---- driver ----

    fn ted_pair(s1: &str, s2: &str, k: i32) -> i32 {
        let mut dict = LabelDict::default();
        let sel = TraversalSelection::default();
        let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
        let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
        ted_k_struct_diff(&t1, &t2, k)
    }

    #[test]
    fn ted_k_cases() {
        assert_eq!(ted_pair("{a{b}{c}}", "{a{b}{c}}", 5), 0);
        assert_eq!(ted_pair("{a{b}{c}}", "{a{b}{x}}", 5), 1);
        assert_eq!(ted_pair("{a}", "{a{b}{c}{d}}", 1), 2);
        assert_eq!(ted_pair("{a{b}{c}}", "{x{y}{z}}", 2), 3);
        assert_eq!(ted_pair("{a{b}{c}}", "{x{y}{z}}", 5), 3);
    }

    // ---- differential test vs Zhang-Shasha oracle (ported from topdiff) ----

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn range(&mut self, n: usize) -> usize {
            (self.next() >> 33) as usize % n
        }
    }

    fn random_tree(rng: &mut Lcg, max_nodes: usize) -> String {
        let labels = ["a", "b", "c", "d"];
        let target = 1 + rng.range(max_nodes);
        let mut remaining = target - 1;
        fn build(rng: &mut Lcg, labels: &[&str], remaining: &mut usize) -> String {
            let mut s = String::from("{");
            s.push_str(labels[rng.range(labels.len())]);
            while *remaining > 0 {
                if rng.range(2) == 0 {
                    break;
                }
                *remaining -= 1;
                s.push_str(&build(rng, labels, remaining));
            }
            s.push('}');
            s
        }
        build(rng, &labels, &mut remaining)
    }

    struct ZsInfo {
        labels: Vec<i32>,
        lmld: Vec<usize>,
        keyroots: Vec<usize>,
    }

    fn zs_info(tree: &ParsedTree) -> ZsInfo {
        fn rec(
            nid: NodeId,
            tree: &ParsedTree,
            labels: &mut Vec<i32>,
            lmld: &mut Vec<usize>,
        ) -> usize {
            let mut first_leaf: Option<usize> = None;
            for cnid in nid.children(tree) {
                let leaf = rec(cnid, tree, labels, lmld);
                first_leaf.get_or_insert(leaf);
            }
            let my_id = labels.len();
            labels.push(*tree.get(nid).unwrap().get());
            lmld.push(first_leaf.unwrap_or(my_id));
            first_leaf.unwrap_or(my_id)
        }

        let root = tree
            .iter()
            .next()
            .and_then(|node| tree.get_node_id(node))
            .expect("oracle needs a non-empty tree");
        let mut labels = Vec::new();
        let mut lmld = Vec::new();
        rec(root, tree, &mut labels, &mut lmld);

        let n = labels.len();
        let keyroots = (0..n)
            .filter(|&i| !(i + 1..n).any(|j| lmld[j] == lmld[i]))
            .collect();
        ZsInfo {
            labels,
            lmld,
            keyroots,
        }
    }

    fn zhang_shasha(t1: &ZsInfo, t2: &ZsInfo) -> i32 {
        let (n, m) = (t1.labels.len(), t2.labels.len());
        let mut td = vec![vec![0i32; m]; n];

        for &i in &t1.keyroots {
            for &j in &t2.keyroots {
                let (li, lj) = (t1.lmld[i], t2.lmld[j]);
                let (rows, cols) = (i - li + 2, j - lj + 2);
                let mut fd = vec![vec![0i32; cols]; rows];
                for r in 1..rows {
                    fd[r][0] = fd[r - 1][0] + 1;
                }
                for c in 1..cols {
                    fd[0][c] = fd[0][c - 1] + 1;
                }
                for r in 1..rows {
                    let di = li + r - 1;
                    for c in 1..cols {
                        let dj = lj + c - 1;
                        if t1.lmld[di] == li && t2.lmld[dj] == lj {
                            let ren = i32::from(t1.labels[di] != t2.labels[dj]);
                            fd[r][c] = (fd[r - 1][c] + 1)
                                .min(fd[r][c - 1] + 1)
                                .min(fd[r - 1][c - 1] + ren);
                            td[di][dj] = fd[r][c];
                        } else {
                            let (fr, fc) = (t1.lmld[di] - li, t2.lmld[dj] - lj);
                            fd[r][c] = (fd[r - 1][c] + 1)
                                .min(fd[r][c - 1] + 1)
                                .min(fd[fr][fc] + td[di][dj]);
                        }
                    }
                }
            }
        }
        td[n - 1][m - 1]
    }

    fn oracle(s1: &str, s2: &str, k: i32) -> i32 {
        let mut dict = LabelDict::default();
        let z1 = zs_info(&pt(s1, &mut dict));
        let z2 = zs_info(&pt(s2, &mut dict));
        let exact = zhang_shasha(&z1, &z2);
        if exact > k {
            k + 1
        } else {
            exact
        }
    }

    fn test_pairs() -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = vec![
            ("{a}".into(), "{a}".into()),
            ("{a{b}{c}}".into(), "{a{b}{c}}".into()),
            ("{a{b{d}}{c}}".into(), "{a{b{d}}{c}}".into()),
            ("{a{b}{c}}".into(), "{a{b}{x}}".into()),
            ("{a{b}}".into(), "{a}".into()),
            ("{a}".into(), "{a{b}}".into()),
            ("{a{b}{c}}".into(), "{a{b}{c}{d}}".into()),
            ("{a}".into(), "{a{b}{c}{d}}".into()),
            ("{a{b{c{d}}}}".into(), "{a}".into()),
            ("{a{b}{c}}".into(), "{x{y}{z}}".into()),
            ("{a{b{c}}}".into(), "{a{b}{c}}".into()),
            ("{r{a}{b}{c}{d}}".into(), "{r{a{b{c{d}}}}}".into()),
            ("{a{b{c}}{d{e}}}".into(), "{a{b{c}{d}}{e}}".into()),
        ];
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        for _ in 0..60 {
            let a = random_tree(&mut rng, 8);
            let b = random_tree(&mut rng, 8);
            pairs.push((a, b));
        }
        pairs
    }

    #[test]
    fn differential_vs_zhang_shasha_oracle() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        let mut checked = 0usize;
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
                let got = ted_k_struct_diff(&t1, &t2, k);
                let want = oracle(s1, s2, k);
                assert_eq!(
                    got, want,
                    "mismatch for s1={s1} s2={s2} k={k}: got={got} oracle={want}"
                );
                checked += 1;
            }
        }
        assert!(checked >= 200, "expected a broad sweep, only checked {checked}");
    }

    /// The research gate: the structural-filtered collection must be a superset
    /// of the always-correct k_relevant-only collection (compared as the set of
    /// emitted top-node pairs, keyed by keyroot ancestors).
    #[test]
    fn harvest_superset_of_full() {
        use std::collections::HashSet;
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);

                let key = |t1: &StructDiffIndex, t2: &StructDiffIndex, v: &[(i32, i32)]| {
                    v.iter()
                        .map(|&(x, y)| {
                            (
                                t1.postl_to_kr_ancestor[x as usize],
                                t2.postl_to_kr_ancestor[y as usize],
                            )
                        })
                        .collect::<HashSet<_>>()
                };
                let full = key(&t1, &t2, &collect_pairs_full(&t1, &t2, k));
                let structf = key(&t1, &t2, &collect_pairs_struct(&t1, &t2, k));
                assert!(
                    full.is_subset(&structf),
                    "struct filter dropped needed top-node pairs for s1={s1} s2={s2} k={k}: \
                     missing={:?}",
                    full.difference(&structf).collect::<Vec<_>>()
                );
            }
        }
    }
}
