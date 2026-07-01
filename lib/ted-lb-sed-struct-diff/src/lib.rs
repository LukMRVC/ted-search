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
//! k-strip guarded by `k_relevant`. Here the SED-STRUCT side supplies the pairs:
//! `harvest_pairs_via_sed_struct` (the default, `USE_SED_HARVEST`) emits a
//! top-node pair for every cell of the SED-STRUCT structural alignment band
//! whose underlying `(x, y)` is structurally admissible (`|sum_x - sum_y| <= k`
//! and `|diff_x - diff_y| <= k`) and `k_relevant`. The annotations are computed
//! in preprocessing and looked up per candidate.
//!
//! ## Why the band is swept rather than the scalar DP instrumented
//! The scalar `bounded_string_edit_distance_with_structure` is a *furthest-
//! reaching* (Berghel-Roach) DP — it touches only the cells it needs to decide
//! the distance, so it cannot enumerate the candidate set (e.g. an 8-node tree
//! vs a single node beelines to the answer and never visits the other keyroots'
//! cells, missing pairs and producing wrong representatives). So harvesting walks
//! the whole `|x - y| <= k` band that the DP occupies and applies the structural
//! admissibility per cell. This admits the same set as `collect_pairs_struct`.
//!
//! ## Exactness
//! `tree_dist` for an outer keyroot pair reads inner subtree distances from the
//! `td` band matrix; those cells exist only if the inner keyroot pair was also
//! collected and processed earlier (pairs run inner-first). So the collected set
//! must be a **superset** of the pairs TopDiff's own `k_relevant` loop would
//! collect. The always-correct `k_relevant`-only collection (`collect_pairs_full`)
//! is kept as the differential-test oracle and runtime fallback; the differential
//! test asserts `collect_pairs_full ⊆ harvest` (and `⊆ collect_pairs_struct`)
//! across a broad random sweep, and checks the driver against an independent
//! Zhang-Shasha oracle. `USE_SED_HARVEST` / `USE_STRUCT_FILTER` select the source.

use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod, TraversalKind, TraversalSelection};
use tree_parsing::{LabelId, ParsedTree};

/// Where the driver gets the top-node pairs it feeds to TopDiff's `tree_dist`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairSource {
    /// TopDiff's exact `k_relevant`-only k-strip scan. Always correct; the oracle.
    Full,
    /// `k_relevant` plus the SED-STRUCT structural admissibility filter (a scan,
    /// no DP). Exact (superset of `Full`); identical to `StructBand`.
    Struct,
    /// Structural-admissibility band sweep. Exact (superset of `Full`).
    StructBand,
    /// **Real SED**: a genuine banded structural string-edit DP (forward+backward)
    /// over the postorder sequences. Its corner cell is the SED lower bound, and
    /// the node pairs lying on some `<= k` alignment are projected to top-node
    /// pairs. NOTE: this is *not* guaranteed to be a superset of the needed pairs,
    /// so the resulting TED can over-estimate (return `k+1` when the true TED is
    /// `<= k`). Measured against the oracle in the tests; not the default.
    SedAlignment,
}

/// Default source feeding `tree_dist`. `StructBand` is exact and the shipped
/// path; flip to `SedAlignment` to drive TopDiff from the real SED alignment.
const PAIR_SOURCE: PairSource = PairSource::StructBand;

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

/// Maximum `e_budget` over all node pairs on the left paths (leftmost-child
/// chains) of the keyroot representative `(x_l, y_l)`. A single
/// `tree_dist(x_l, y_l, e_max)` fills the `td` cells for every left-path node
/// pair, each of which an enclosing keyroot pair may later read; provisioning
/// the call with only the representative's `e_budget(x_l, y_l, k)`
/// under-budgets those inner cells and yields `k+1` when the true TED is `<= k`.
/// Ports the `compute_e_max` branch of the C++ `touzet_kr_set_tree_index_impl.h`.
fn e_max_over_left_paths(
    t1: &StructDiffIndex,
    t2: &StructDiffIndex,
    x_l: i32,
    y_l: i32,
    k: i32,
) -> i32 {
    let mut e_max = 0;
    let mut top_x = x_l;
    while top_x > -1 {
        let mut top_y = y_l;
        while top_y > -1 {
            e_max = e_max.max(e_budget(t1, t2, top_x, top_y, k));
            top_y = t2.postl_to_lch[top_y as usize];
        }
        top_x = t1.postl_to_lch[top_x as usize];
    }
    e_max
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
                let key = ((x_keyroot as u64) << 32) | (t2.postl_to_kr_ancestor[y as usize] as u64);
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
    br_sed_impl(s1, s2, k, |_, _| {})
}

/// The matrix cells `(row, col)` that the furthest-reaching BR-SED DP *examines*
/// (compares a character pair for) between `s1` and `s2` at threshold `k` — the
/// sparse "beeline" Berghel-Roach actually touches, always within the
/// `|col - row| <= k` band. Same precondition as
/// [`bounded_string_edit_distance_with_structure`]: `s2.len() >= s1.len()`.
pub fn br_sed_examined_cells(
    s1: &[TraversalCharacter],
    s2: &[TraversalCharacter],
    k: usize,
) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    br_sed_impl(s1, s2, k, |r, c| cells.push((r, c)));
    cells
}

/// BR-SED core, generic over a per-examined-cell visitor so callers can either
/// ignore it (the scalar distance) or harvest the examined cells. `on_examine`
/// is called with `(row, col)` for every character comparison the snake makes;
/// those indices are always in bounds. With a no-op visitor this monomorphises
/// back to the original scalar DP at zero cost.
fn br_sed_impl<F: FnMut(i32, i32)>(
    s1: &[TraversalCharacter],
    s2: &[TraversalCharacter],
    k: usize,
    mut on_examine: F,
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
                    on_examine(max_row_number, max_row_number + diag_offset);

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
// SED-STRUCT pair harvesting: the banded structural string-edit DP emits the
// top-node pairs directly, instead of enumerating the k-strip.
// ===========================================================================

/// Dedup a candidate node pair `(pa, pb)` — postorder ids in tree `a`/`b` (the
/// DP orientation, possibly swapped relative to `t1`/`t2`) — into `vec` keyed by
/// its top-node (keyroot-ancestor) pair, keeping the representative with the
/// largest `x` and `y` (matching `collect_pairs`: largest `x` is the keyroot
/// itself; largest `y` makes the forest extent reach far enough for `tree_dist`).
#[allow(clippy::too_many_arguments)]
#[inline]
fn emit_pair(
    t1: &StructDiffIndex,
    t2: &StructDiffIndex,
    k: i32,
    swapped: bool,
    pa: i32,
    pb: i32,
    map: &mut rustc_hash::FxHashMap<u64, usize>,
    vec: &mut Vec<(i32, i32)>,
) {
    let (x, y) = if swapped { (pb, pa) } else { (pa, pb) };
    // Same admission test as TopDiff's scan: a cell only contributes a top-node
    // pair if its underlying `(x, y)` is k-relevant. This is what keeps the
    // representative valid — `k_relevant` implies `|x - y| <= k`, so the
    // independent max-x / max-y dedup below stays inside the band, exactly as in
    // `collect_pairs`.
    if !k_relevant(t1, t2, x, y, k) {
        return;
    }
    let kx = t1.postl_to_kr_ancestor[x as usize];
    let ky = t2.postl_to_kr_ancestor[y as usize];
    let key = ((kx as u64) << 32) | (ky as u64);
    match map.get(&key) {
        None => {
            map.insert(key, vec.len());
            vec.push((x, y));
        }
        Some(&idx) => {
            if x > vec[idx].0 {
                vec[idx].0 = x;
            }
            if y > vec[idx].1 {
                vec[idx].1 = y;
            }
        }
    }
}

/// Emit the top-node pairs of the SED-STRUCT structural alignment band over the
/// two **postorder** sequences. Returns the deduped `kr_vector` of representative
/// `(x, y)` node pairs.
///
/// ## Why a full band sweep, not the scalar Berghel-Roach DP
/// `bounded_string_edit_distance_with_structure` is a *furthest-reaching*
/// (Berghel-Roach) DP: it only ever touches the handful of cells it needs to
/// decide the distance, so it cannot enumerate the candidate set — for e.g. an
/// 8-node tree vs a single node it beelines to the answer and never visits the
/// cells of the other keyroots, missing top-node pairs and producing wrong
/// representatives. Instrumenting its snake is therefore not viable.
///
/// Instead we walk the **whole** `|x - y| <= k` band (the same band that DP
/// occupies) over the postorder sequences and emit a top-node pair for every
/// cell whose `(x, y)` is structurally admissible — `|sum_x - sum_y| <= k` and
/// `|diff_x - diff_y| <= k`, the loosest form of the snake's own per-character
/// structural test (`bounded_string_edit_distance_with_structure`). Because we
/// sweep `x` in decreasing postorder and `emit_pair` keeps the largest `x` and
/// `y` per keyroot pair, the representatives match TopDiff's scan exactly. On
/// postorder a traversal position *is* a postorder id, so no position→id map is
/// needed. (This admits the same set as `collect_pairs_struct`; the structural
/// admissibility is what SED-STRUCT contributes over the plain k-strip.)
fn harvest_pairs_via_sed_struct(
    t1: &StructDiffIndex,
    t2: &StructDiffIndex,
    k: i32,
) -> Vec<(i32, i32)> {
    use rustc_hash::FxHashMap;

    let mut map: FxHashMap<u64, usize> = FxHashMap::default();
    let mut kr_vector: Vec<(i32, i32)> = Vec::new();

    for x in (0..t1.tree_size).rev() {
        let c1 = &t1.postl_struct[x as usize];
        let y_hi = (x + k).min(t2.tree_size - 1);
        let y_lo = (0).max(x - k);
        for y in (y_lo..=y_hi).rev() {
            let c2 = &t2.postl_struct[y as usize];
            // SED-STRUCT structural admissibility (the snake's per-character test
            // at zero accumulated edits).
            if (c1.sum - c2.sum).abs() <= k && (c1.diff - c2.diff).abs() <= k {
                // swapped = false: x indexes t1, y indexes t2 directly.
                emit_pair(t1, t2, k, false, x, y, &mut map, &mut kr_vector);
            }
        }
    }

    kr_vector
}

// ===========================================================================
// Real SED: a genuine banded structural string-edit DP over the postorder
// sequences. Forward + backward passes give the SED lower bound and let us read
// off the node pairs that lie on some `<= k` structural alignment.
// ===========================================================================

/// Sentinel for "cost already exceeds `k`" — keeps the DP in `i32` without
/// overflow while capping anything past the budget.
const SED_INF: i32 = 1 << 28;

/// Substitution is permitted only between structurally compatible nodes (the
/// loosest, position-independent form of the SED-STRUCT per-character test).
#[inline]
fn sed_sub_compatible(c1: &TraversalCharacter, c2: &TraversalCharacter, k: i32) -> bool {
    (c1.sum - c2.sum).abs() <= k && (c1.diff - c2.diff).abs() <= k
}

/// Forward structural string-edit DP. `f[i][j]` = structural SED between the
/// postorder prefixes `s1[..i]` and `s2[..j]`, capped to `SED_INF` once it passes
/// `k`. Diagonal (substitution) moves are allowed only between structurally
/// compatible nodes — cost `0` if labels match, else `1` (rename); incompatible
/// nodes must be aligned via indels. `f[n1][n2]` is the whole-sequence SED LB.
fn sed_forward(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: i32) -> Vec<Vec<i32>> {
    let (n1, n2) = (s1.len(), s2.len());
    let mut f = vec![vec![SED_INF; n2 + 1]; n1 + 1];
    f[0][0] = 0;
    for i in 0..=n1 {
        for j in 0..=n2 {
            if i == 0 && j == 0 {
                continue;
            }
            let mut best = SED_INF;
            if i > 0 {
                best = best.min(f[i - 1][j] + 1);
            }
            if j > 0 {
                best = best.min(f[i][j - 1] + 1);
            }
            if i > 0 && j > 0 && f[i - 1][j - 1] < SED_INF {
                let (c1, c2) = (&s1[i - 1], &s2[j - 1]);
                if sed_sub_compatible(c1, c2, k) {
                    let sub = i32::from(c1.char != c2.char);
                    best = best.min(f[i - 1][j - 1] + sub);
                }
            }
            f[i][j] = if best > k { SED_INF } else { best };
        }
    }
    f
}

/// Backward structural string-edit DP. `b[i][j]` = structural SED between the
/// postorder suffixes `s1[i..]` and `s2[j..]`. Same recurrence as `sed_forward`,
/// filled from the bottom-right.
fn sed_backward(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: i32) -> Vec<Vec<i32>> {
    let (n1, n2) = (s1.len(), s2.len());
    let mut b = vec![vec![SED_INF; n2 + 1]; n1 + 1];
    b[n1][n2] = 0;
    for i in (0..=n1).rev() {
        for j in (0..=n2).rev() {
            if i == n1 && j == n2 {
                continue;
            }
            let mut best = SED_INF;
            if i < n1 {
                best = best.min(b[i + 1][j] + 1);
            }
            if j < n2 {
                best = best.min(b[i][j + 1] + 1);
            }
            if i < n1 && j < n2 && b[i + 1][j + 1] < SED_INF {
                let (c1, c2) = (&s1[i], &s2[j]);
                if sed_sub_compatible(c1, c2, k) {
                    let sub = i32::from(c1.char != c2.char);
                    best = best.min(b[i + 1][j + 1] + sub);
                }
            }
            b[i][j] = if best > k { SED_INF } else { best };
        }
    }
    b
}

/// The real SED computation, serving both roles: returns the SED **lower bound**
/// `f[n1][n2]` (capped to `k+1`) and the **top-node pairs** generated from the
/// alignment. A node pair `(x, y)` is emitted when it sits on some `<= k`
/// structural alignment — i.e. `f[x][y] + sub(x,y) + b[x+1][y+1] <= k` — and is
/// projected to its top-node pair via `emit_pair` (`k_relevant` + dedup).
///
/// Runs on the **postorder** sequences (position == postorder id), so emitted
/// indices feed `tree_dist` directly. Sweeping `x` in decreasing postorder makes
/// the dedup pick the same representative form as TopDiff.
fn sed_alignment(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> (i32, Vec<(i32, i32)>) {
    use rustc_hash::FxHashMap;

    let s1 = &t1.postl_struct;
    let s2 = &t2.postl_struct;
    let (n1, n2) = (s1.len(), s2.len());

    let f = sed_forward(s1, s2, k);
    let lb = f[n1][n2].min(k + 1);
    if lb > k {
        return (lb, Vec::new());
    }

    let b = sed_backward(s1, s2, k);
    let mut map: FxHashMap<u64, usize> = FxHashMap::default();
    let mut kr_vector: Vec<(i32, i32)> = Vec::new();

    for i in (1..=n1).rev() {
        for j in (1..=n2).rev() {
            let (c1, c2) = (&s1[i - 1], &s2[j - 1]);
            if !sed_sub_compatible(c1, c2, k) {
                continue;
            }
            let before = f[i - 1][j - 1];
            let after = b[i][j];
            if before >= SED_INF || after >= SED_INF {
                continue;
            }
            let sub = i32::from(c1.char != c2.char);
            if before + sub + after <= k {
                // node x = i-1 in t1, y = j-1 in t2 lie on a <= k alignment.
                emit_pair(
                    t1,
                    t2,
                    k,
                    false,
                    (i - 1) as i32,
                    (j - 1) as i32,
                    &mut map,
                    &mut kr_vector,
                );
            }
        }
    }

    (lb, kr_vector)
}

// ===========================================================================
// Driver.
// ===========================================================================

/// Optional per-phase profiling. Enable with `--features profile-phases`;
/// without the feature every hook below compiles to nothing (zero overhead).
///
/// Usage:
/// ```ignore
/// use ted_lb_sed_struct_diff::phase_timing;
/// phase_timing::reset();
/// // ... run many ted_k_struct_diff calls (single- or multi-threaded) ...
/// let t = phase_timing::snapshot();
/// println!("prune {:?}  collect {:?}  topdiff {:?}", t.prune, t.collect, t.topdiff);
/// ```
#[cfg(feature = "profile-phases")]
pub mod phase_timing {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    use std::time::Duration;

    pub(crate) static PRUNE_NS: AtomicU64 = AtomicU64::new(0);
    pub(crate) static COLLECT_NS: AtomicU64 = AtomicU64::new(0);
    pub(crate) static TOPDIFF_NS: AtomicU64 = AtomicU64::new(0);

    /// Nanoseconds accumulated in each phase since the last [`reset`], summed
    /// over every `ted_k_with_source` call and every thread.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct PhaseTimings {
        pub prune: Duration,
        pub collect: Duration,
        pub topdiff: Duration,
    }

    /// Zero the accumulators before a measured workload.
    pub fn reset() {
        PRUNE_NS.store(0, Relaxed);
        COLLECT_NS.store(0, Relaxed);
        TOPDIFF_NS.store(0, Relaxed);
    }

    /// Read the totals accumulated since the last [`reset`].
    pub fn snapshot() -> PhaseTimings {
        PhaseTimings {
            prune: Duration::from_nanos(PRUNE_NS.load(Relaxed)),
            collect: Duration::from_nanos(COLLECT_NS.load(Relaxed)),
            topdiff: Duration::from_nanos(TOPDIFF_NS.load(Relaxed)),
        }
    }
}

/// Run `$body`, adding its wall-clock time to the named phase accumulator when
/// `profile-phases` is on. Without the feature it expands to just `$body`.
macro_rules! timed_phase {
    ($acc:ident, $body:block) => {{
        #[cfg(feature = "profile-phases")]
        let __start = std::time::Instant::now();
        let __result = $body;
        #[cfg(feature = "profile-phases")]
        phase_timing::$acc.fetch_add(
            __start.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        __result
    }};
}

/// Exact bounded TED via TopDiff with SED-STRUCT-derived top-node pairs.
/// Returns the exact TED when `<= k`, otherwise `k + 1`. Uses the default
/// [`PAIR_SOURCE`].
pub fn ted_k_struct_diff(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> i32 {
    ted_k_with_source(t1, t2, k, PAIR_SOURCE)
}

/// Driver parameterised by the top-node pair source (so tests can exercise each).
pub fn ted_k_with_source(
    t1: &StructDiffIndex,
    t2: &StructDiffIndex,
    k: i32,
    source: PairSource,
) -> i32 {
    let t1_size = t1.tree_size;
    let t2_size = t2.tree_size;

    if (t1_size - t2_size).abs() > k {
        return k + 1;
    }

    // Phase 1: early prune via the SED-STRUCT structural lower bound.
    let pruned = timed_phase!(PRUNE_NS, { sed_struct_lb(t1, t2, k as usize) > k as usize });
    if pruned {
        return k + 1;
    }

    // Phase 2: collect the top-node pairs and sort them inner-first (ascending
    // (x, y), so every inner keyroot pair is processed before any outer pair
    // that reads its td cell).
    let pairs = timed_phase!(COLLECT_NS, {
        let mut pairs = match source {
            PairSource::Full => collect_pairs_full(t1, t2, k),
            PairSource::Struct => collect_pairs_struct(t1, t2, k),
            PairSource::StructBand => harvest_pairs_via_sed_struct(t1, t2, k),
            PairSource::SedAlignment => sed_alignment(t1, t2, k).1,
        };
        pairs.sort_unstable();
        pairs
    });

    // Phase 3: TopDiff forest DP over the collected pairs.
    let result = timed_phase!(TOPDIFF_NS, {
        let mut state = TopDiffState::new(t1_size, k);
        for &(x_l, y_l) in &pairs {
            // e_max must cover every inner (left-path) node pair this forest DP
            // computes, not just the representative; see `e_max_over_left_paths`.
            let e_max = e_max_over_left_paths(t1, t2, x_l, y_l, k);
            let d = state.tree_dist(t1, t2, x_l, y_l, k, e_max);
            state.td.set(x_l as usize, y_l as usize, d);
        }
        state
            .td
            .read_at((t1_size - 1) as usize, (t2_size - 1) as usize)
    });

    if !result.is_finite() || result > k as f64 {
        return k + 1;
    }
    result as i32
}

// ===========================================================================
// TopNodes matrix: visualization data for the top-node-pair figure.
// ===========================================================================

/// The overlaid layers of the "Top node pairs" figure for a tree pair at
/// threshold `k`. Rows are indexed by `x` (postorder id in `t1`), columns by `y`
/// (postorder id in `t2`); every `Vec<Vec<bool>>` is `n1` rows of `n2` columns.
///
/// * `in_band` — `|x - y| <= k` (the k-strip / postorder difference, light gray).
/// * `struct_ok` — the SED-STRUCT neighborhood test `|sum_x - sum_y| <= k &&
///   |diff_x - diff_y| <= k` (dark gray).
/// * `k_relevant` — the `k_relevant` predicate (an extra diagnostic layer).
/// * `is_topnode` — membership in the deduped top-node representative set fed to
///   `tree_dist` for the chosen [`PairSource`] (green).
///
/// The harvest dedup keeps the independent max-`x` and max-`y` per keyroot pair,
/// so an `is_topnode` cell can occasionally fall outside `struct_ok`; the layers
/// are emitted faithfully rather than forcing green ⊆ dark gray.
#[derive(Debug, Clone)]
pub struct TopNodeMatrix {
    pub k: i32,
    pub n1: i32,
    pub n2: i32,
    /// `t1` postorder label ids (row labels `x0..x_{n1-1}`).
    pub x_labels: Vec<i32>,
    /// `t2` postorder label ids (column labels `y0..y_{n2-1}`).
    pub y_labels: Vec<i32>,
    pub in_band: Vec<Vec<bool>>,
    pub struct_ok: Vec<Vec<bool>>,
    pub k_relevant: Vec<Vec<bool>>,
    pub is_topnode: Vec<Vec<bool>>,
    /// The cells the furthest-reaching BR-SED DP examines over the two postorder
    /// sequences (the sparse "beeline"). Always `⊆ in_band`; contrasts with the
    /// full band sweep the harvest performs.
    pub br_sed_visited: Vec<Vec<bool>>,
    /// The raw representative `(x, y)` pairs fed to `tree_dist` for `source`.
    pub topnode_pairs: Vec<(i32, i32)>,
}

/// Compute the [`TopNodeMatrix`] layers for `t1` vs `t2` at threshold `k`, with
/// the top-node (green) layer taken from the given [`PairSource`] (`StructBand`
/// is the shipped harvest). Pure and deterministic.
pub fn topnode_matrix(
    t1: &StructDiffIndex,
    t2: &StructDiffIndex,
    k: i32,
    source: PairSource,
) -> TopNodeMatrix {
    let n1 = t1.tree_size;
    let n2 = t2.tree_size;

    let mut in_band = vec![vec![false; n2 as usize]; n1 as usize];
    let mut struct_ok = vec![vec![false; n2 as usize]; n1 as usize];
    let mut k_relevant_grid = vec![vec![false; n2 as usize]; n1 as usize];
    let mut is_topnode = vec![vec![false; n2 as usize]; n1 as usize];

    for x in 0..n1 {
        for y in 0..n2 {
            in_band[x as usize][y as usize] = (x - y).abs() <= k;
            struct_ok[x as usize][y as usize] = struct_pair_ok(t1, t2, x, y, k);
            k_relevant_grid[x as usize][y as usize] = k_relevant(t1, t2, x, y, k);
        }
    }

    let topnode_pairs = match source {
        PairSource::Full => collect_pairs_full(t1, t2, k),
        PairSource::Struct => collect_pairs_struct(t1, t2, k),
        PairSource::StructBand => harvest_pairs_via_sed_struct(t1, t2, k),
        PairSource::SedAlignment => sed_alignment(t1, t2, k).1,
    };
    for &(x, y) in &topnode_pairs {
        is_topnode[x as usize][y as usize] = true;
    }

    // BR-SED examined cells over the postorder sequences. BR-SED requires
    // `s2.len() >= s1.len()` (orient by size, transpose the DP coords back when
    // swapped) and `|n1 - n2| <= k` — the same size prune `sed_struct_lb` applies
    // before ever calling it; when that prune would fire, BR-SED never runs, so
    // the layer stays empty, faithfully.
    let mut br_sed_visited = vec![vec![false; n2 as usize]; n1 as usize];
    if k >= 0 && (n1 - n2).abs() <= k {
        let (s_t1, s_t2) = (&t1.postl_struct, &t2.postl_struct);
        let swapped = s_t1.len() > s_t2.len();
        let (a, b) = if swapped { (s_t2, s_t1) } else { (s_t1, s_t2) };
        for (row, col) in br_sed_examined_cells(a, b, k as usize) {
            // `a` is the shorter side: row indexes `a`, col indexes `b`.
            let (x, y) = if swapped { (col, row) } else { (row, col) };
            br_sed_visited[x as usize][y as usize] = true;
        }
    }

    TopNodeMatrix {
        k,
        n1,
        n2,
        x_labels: t1.postl_to_label_id.clone(),
        y_labels: t2.postl_to_label_id.clone(),
        in_band,
        struct_ok,
        k_relevant: k_relevant_grid,
        is_topnode,
        br_sed_visited,
        topnode_pairs,
    }
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
        assert_eq!(
            idx.postl_struct[0],
            TraversalCharacter {
                char: idx.postl_struct[0].char,
                sum: 2,
                diff: 0
            }
        );
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
            // Regression for the left-path e_max under-provisioning bug (true TED
            // == k at the boundary returned k+1). See `e_max_over_left_paths`.
            ("{a{b}}".into(), "{a{c}{b{a}}}".into()),
            ("{c{b}{a{a}}}".into(), "{c{d{c}}}".into()),
            ("{b{b{b}{c{d}}}}".into(), "{b{c}}".into()),
        ];
        // Deeper trees (was 8): the left-path budget gap grows with depth, so the
        // boundary bug only surfaces on deeper inputs.
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        for _ in 0..200 {
            let a = random_tree(&mut rng, 22);
            let b = random_tree(&mut rng, 22);
            pairs.push((a, b));
        }
        pairs
    }

    #[test]
    fn differential_vs_zhang_shasha_oracle() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 4, 5, 8, 50];
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
        assert!(
            checked >= 200,
            "expected a broad sweep, only checked {checked}"
        );
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
                let harvested = key(&t1, &t2, &harvest_pairs_via_sed_struct(&t1, &t2, k));
                assert!(
                    full.is_subset(&structf),
                    "struct filter dropped needed top-node pairs for s1={s1} s2={s2} k={k}: \
                     missing={:?}",
                    full.difference(&structf).collect::<Vec<_>>()
                );
                assert!(
                    full.is_subset(&harvested),
                    "SED harvest dropped needed top-node pairs for s1={s1} s2={s2} k={k}: \
                     missing={:?}",
                    full.difference(&harvested).collect::<Vec<_>>()
                );
            }
        }
    }

    /// The visualization matrix must reproduce the crate's own predicates
    /// cell-by-cell, and its green layer must be exactly the harvest set.
    #[test]
    fn topnode_matrix_layers_match_predicates() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
                let m = topnode_matrix(&t1, &t2, k, PairSource::StructBand);

                assert_eq!(m.n1, t1.tree_size);
                assert_eq!(m.n2, t2.tree_size);
                assert_eq!(m.x_labels, t1.postl_to_label_id);
                assert_eq!(m.y_labels, t2.postl_to_label_id);

                for x in 0..t1.tree_size {
                    for y in 0..t2.tree_size {
                        let (xi, yi) = (x as usize, y as usize);
                        assert_eq!(m.in_band[xi][yi], (x - y).abs() <= k);
                        assert_eq!(m.struct_ok[xi][yi], struct_pair_ok(&t1, &t2, x, y, k));
                        assert_eq!(m.k_relevant[xi][yi], k_relevant(&t1, &t2, x, y, k));
                    }
                }

                // Green layer == the harvested representative set exactly.
                let harvested: std::collections::HashSet<(i32, i32)> =
                    harvest_pairs_via_sed_struct(&t1, &t2, k)
                        .into_iter()
                        .collect();
                let green: std::collections::HashSet<(i32, i32)> = (0..t1.tree_size)
                    .flat_map(|x| (0..t2.tree_size).map(move |y| (x, y)))
                    .filter(|&(x, y)| m.is_topnode[x as usize][y as usize])
                    .collect();
                assert_eq!(
                    green, harvested,
                    "green != harvest for s1={s1} s2={s2} k={k}"
                );
                assert_eq!(
                    m.topnode_pairs
                        .iter()
                        .cloned()
                        .collect::<std::collections::HashSet<_>>(),
                    harvested
                );
            }
        }
    }

    /// BR-SED is diagonal-banded, so every cell it examines must lie in the
    /// `|x-y| <= k` band shown as light gray.
    #[test]
    fn br_sed_visited_within_band() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
                let m = topnode_matrix(&t1, &t2, k, PairSource::StructBand);
                for x in 0..t1.tree_size {
                    for y in 0..t2.tree_size {
                        if m.br_sed_visited[x as usize][y as usize] {
                            assert!(
                                m.in_band[x as usize][y as usize],
                                "BR-SED examined out-of-band cell ({x},{y}) \
                                 s1={s1} s2={s2} k={k}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// For two identical trees the snake runs straight down the main diagonal, so
    /// every `(i, i)` cell must be examined.
    #[test]
    fn br_sed_visited_covers_diagonal_for_equal_trees() {
        let sel = TraversalSelection::default();
        for s in ["{a}", "{a{b}{c}}", "{a{b{d}}{c}}", "{a{b{c{d}}}}", "{a{b}{c}{d}}"] {
            let mut dict = LabelDict::default();
            let t1 = StructDiffIndex::from_tree(&pt(s, &mut dict), sel);
            let t2 = StructDiffIndex::from_tree(&pt(s, &mut dict), sel);
            let n = t1.tree_size;
            let m = topnode_matrix(&t1, &t2, n, PairSource::StructBand);
            for i in 0..n {
                assert!(
                    m.br_sed_visited[i as usize][i as usize],
                    "diagonal cell ({i},{i}) not examined for equal tree {s}"
                );
            }
        }
    }

    /// TED is symmetric, and harvesting must not depend on argument order: the
    /// driver must return the same distance with the trees swapped (exercises the
    /// representative bookkeeping from both orientations).
    #[test]
    fn driver_is_swap_invariant() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
                let ab = ted_k_struct_diff(&t1, &t2, k);
                let ba = ted_k_struct_diff(&t2, &t1, k);
                assert_eq!(ab, ba, "swap mismatch s1={s1} s2={s2} k={k}: {ab} vs {ba}");
            }
        }
    }

    /// The real SED forward DP must be a sound lower bound: `f[n1][n2] <= TED`.
    #[test]
    fn sed_forward_is_sound_lower_bound() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
                let f = sed_forward(&t1.postl_struct, &t2.postl_struct, k);
                let lb = f[t1.postl_struct.len()][t2.postl_struct.len()].min(k + 1);
                let exact = oracle(s1, s2, k); // capped to k+1
                assert!(
                    lb <= exact || (exact == k + 1 && lb == k + 1),
                    "SED LB {lb} exceeds TED {exact} for s1={s1} s2={s2} k={k}"
                );
            }
        }
    }

    /// Measure the `SedAlignment` source against the oracle: it must never
    /// *under*-estimate (soundness — a finite result must equal the true TED),
    /// but it may over-estimate (return `k+1` when the true TED is `<= k`) when
    /// the alignment misses a needed top-node pair. Prints the false-negative
    /// rate so we can see empirically how usable SED-driven generation is.
    #[test]
    fn measure_sed_alignment_vs_oracle() {
        let pairs = test_pairs();
        let ks = [1, 2, 3, 50];
        let sel = TraversalSelection::default();
        let (mut total, mut exact_hits, mut false_negs) = (0usize, 0usize, 0usize);
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = StructDiffIndex::from_tree(&pt(s1, &mut dict), sel);
                let t2 = StructDiffIndex::from_tree(&pt(s2, &mut dict), sel);
                let got = ted_k_with_source(&t1, &t2, k, PairSource::SedAlignment);
                let want = oracle(s1, s2, k);
                total += 1;
                if got == want {
                    exact_hits += 1;
                } else {
                    // The only acceptable disagreement is an over-estimate.
                    assert!(
                        got > want && want <= k,
                        "SedAlignment UNDER-estimated (unsound): got={got} want={want} \
                         s1={s1} s2={s2} k={k}"
                    );
                    false_negs += 1;
                }
            }
        }
        println!(
            "SedAlignment: {exact_hits}/{total} exact, {false_negs} false-negatives \
             ({:.1}% miss rate)",
            100.0 * false_negs as f64 / total as f64
        );
    }
}
