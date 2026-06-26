//! Rust port of the C++ Touzet KR-set bounded tree edit distance (TopDiff).
//!
//! Differential-tested against an independent Zhang-Shasha exact-TED oracle
//! (see the test module), replacing the C++ FFI oracle of the source repo.

use indextree::NodeId;
use tree_parsing::ParsedTree;

/// Rust equivalent of C++ `node::TreeIndexTouzetKRSet`. All vectors are indexed
/// by left-to-right POSTORDER id (`0..tree_size`).
///
/// INVARIANTS:
///   * `postl_to_label_id`: label ids as interned by `tree_parsing`'s shared
///     `LabelDict` at parse time (so ids are comparable across trees parsed
///     against the same dict)
///   * `postl_to_size`: subtree node count; leaf == 1
///   * `postl_to_depth`: root depth == 0
///   * `postl_to_lch`: leftmost-child postorder id; LEAF == -1
///   * `list_kr`: postorder ids of keyroots (non-first children + root); order irrelevant
///   * `postl_to_kr_ancestor`: nearest keyroot ancestor postorder id
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopDiffIndex {
    pub tree_size: i32,
    pub postl_to_label_id: Vec<i32>,
    pub postl_to_size: Vec<i32>,
    pub postl_to_depth: Vec<i32>,
    pub postl_to_lch: Vec<i32>,
    pub postl_to_kr_ancestor: Vec<i32>,
    pub list_kr: Vec<i32>,
}

/// A specialised matrix where only the elements on a diagonal band matter.
/// Port of the C++ `data_structures::BandMatrix<double>` (`include/matrix.h`).
///
/// The backing store is a flat `rows * (2 * band_width + 1)` vector. Accessing
/// `(row, col)` translates the column to `col + band_width - row` (the diagonal
/// shift that reduces memory from `O(n^2)` to `O(n*k)`). Callers must ensure all
/// accessed cells are within the band, exactly as in the C++ version.
#[derive(Debug, Clone)]
pub struct BandMatrix {
    columns: usize,
    band_width: usize,
    data: Vec<f64>,
}

impl BandMatrix {
    /// Mirrors `BandMatrix(rows, band_width)` followed by `fill_with(fill)`.
    /// Backing matrix has `2 * band_width + 1` columns.
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
        // C++: Matrix::at(row, col + band_width_ - row). The column translation
        // can transiently underflow in usize, so compute in isize then index.
        let translated = col as isize + self.band_width as isize - row as isize;
        row * self.columns + translated as usize
    }

    /// `BandMatrix::at(row, col)` for writing.
    #[inline]
    pub fn set(&mut self, row: usize, col: usize, value: f64) {
        let idx = self.translate(row, col);
        self.data[idx] = value;
    }

    /// Mutable reference equivalent to C++ `at(row, col)`. Mirrors the C++ API
    /// surface; the DP uses `set`/`read_at`, so this is exercised only by tests.
    #[allow(dead_code)]
    #[inline]
    pub fn at(&mut self, row: usize, col: usize) -> &mut f64 {
        let idx = self.translate(row, col);
        &mut self.data[idx]
    }

    /// `BandMatrix::read_at(row, col)` — const read.
    #[inline]
    pub fn read_at(&self, row: usize, col: usize) -> f64 {
        let idx = self.translate(row, col);
        self.data[idx]
    }
}

impl TopDiffIndex {
    /// Builder. Ports `node::index_tree` / `index_tree_recursion` for the
    /// `TreeIndexTouzetKRSet` subset of indices, plus `fill_kr_ancestors`.
    ///
    /// `ParsedTree` nodes already hold `LabelId`s interned by `tree_parsing`
    /// against a shared `LabelDict`, so no dict is threaded through here —
    /// label ids are copied verbatim into `postl_to_label_id`.
    pub fn from_tree(tree: &ParsedTree) -> Self {
        let tree_size = tree.count();

        let mut postl_to_label_id = vec![0i32; tree_size];
        let mut postl_to_size = vec![0i32; tree_size];
        let mut postl_to_depth = vec![0i32; tree_size];
        let mut postl_to_lch = vec![-1i32; tree_size];
        let mut list_kr: Vec<i32> = Vec::new();

        // Running postorder id; incremented after a node and all its children
        // are processed (mirrors `start_postorder`).
        let mut next_postorder: i32 = 0;

        // First node in insertion order is the root (matches `tree_to_bracket`).
        let root = tree.iter().next().and_then(|node| tree.get_node_id(node));
        if let Some(root) = root {
            Self::recurse(
                root,
                tree,
                0, // start_depth: root depth == 0
                &mut next_postorder,
                &mut postl_to_label_id,
                &mut postl_to_size,
                &mut postl_to_depth,
                &mut postl_to_lch,
                &mut list_kr,
            );
            // Root is appended to the keyroot list last (C++: list_kr_.push_back(start_postorder - 1)).
            list_kr.push(next_postorder - 1);
        }

        // fill_kr_ancestors: walk the left-path (lch chain) from each keyroot.
        let mut postl_to_kr_ancestor = vec![0i32; tree_size];
        for &i in &list_kr {
            let mut l = i;
            while l >= 0 {
                postl_to_kr_ancestor[l as usize] = i;
                l = postl_to_lch[l as usize];
            }
        }

        Self {
            tree_size: tree_size as i32,
            postl_to_label_id,
            postl_to_size,
            postl_to_depth,
            postl_to_lch,
            postl_to_kr_ancestor,
            list_kr,
        }
    }

    /// Postorder DFS. Returns the subtree size rooted at `nid` and assigns this
    /// node's postorder id, label, size, depth, lch; pushes non-first children
    /// to `list_kr`.
    #[allow(clippy::too_many_arguments)]
    fn recurse(
        nid: NodeId,
        tree: &ParsedTree,
        depth: i32,
        next_postorder: &mut i32,
        postl_to_label_id: &mut [i32],
        postl_to_size: &mut [i32],
        postl_to_depth: &mut [i32],
        postl_to_lch: &mut [i32],
        list_kr: &mut Vec<i32>,
    ) -> i32 {
        let mut desc_sum = 0i32;
        let mut first_child_postorder: i32 = -1;

        let mut is_first = true;
        for cnid in nid.children(tree) {
            let child_size = Self::recurse(
                cnid,
                tree,
                depth + 1,
                next_postorder,
                postl_to_label_id,
                postl_to_size,
                postl_to_depth,
                postl_to_lch,
                list_kr,
            );
            desc_sum += child_size;
            // The child's postorder id is `*next_postorder - 1` after its recursion.
            let child_postorder = *next_postorder - 1;
            if is_first {
                first_child_postorder = child_postorder;
                is_first = false;
            } else {
                // Non-first children are keyroots.
                list_kr.push(child_postorder);
            }
        }

        // Now *next_postorder holds this node's postorder id.
        let this_postorder = *next_postorder as usize;

        // Nodes already carry the interned `LabelId`.
        postl_to_label_id[this_postorder] = *tree.get(nid).unwrap().get();
        postl_to_size[this_postorder] = desc_sum + 1;
        postl_to_depth[this_postorder] = depth;
        postl_to_lch[this_postorder] = first_child_postorder;

        *next_postorder += 1;
        desc_sum + 1
    }
}

/// Remaining error budget for the subtree pair `(x, y)`.
/// Port of `TEDAlgorithmTouzet::e_budget` (`ted_algorithm_touzet.h:285`).
///
/// `e(x,y) = k - |(|T1|-(x+1)-depth(x)) - (|T2|-(y+1)-depth(y))|`
///            `- |depth(x)-depth(y)| - |((x+1)-|T1_x|) - ((y+1)-|T2_y|)|`
///
/// May return a NEGATIVE value — callers must not clamp (matches the C++ note).
pub fn e_budget(t1: &TopDiffIndex, t2: &TopDiffIndex, x: i32, y: i32, k: i32) -> i32 {
    let x_size = t1.postl_to_size[x as usize];
    let y_size = t2.postl_to_size[y as usize];
    let dx = t1.postl_to_depth[x as usize];
    let dy = t2.postl_to_depth[y as usize];
    let lower_bound = ((t1.tree_size - (x + 1) - dx) - (t2.tree_size - (y + 1) - dy)).abs()
        + (dx - dy).abs()
        + (((x + 1) - x_size) - ((y + 1) - y_size)).abs();
    k - lower_bound
}

/// Whether subtrees `T1_x` and `T2_y` are k-relevant.
/// Port of `TEDAlgorithmTouzet::k_relevant` (`ted_algorithm_touzet.h:315`).
///
/// True iff `|(|T1|-(x+1)-depth(x)) - (|T2|-(y+1)-depth(y))| + |depth(x)-depth(y)|`
///          `+ ||T1_x|-|T2_y|| + |((x+1)-|T1_x|) - ((y+1)-|T2_y|)| <= k`.
pub fn k_relevant(t1: &TopDiffIndex, t2: &TopDiffIndex, x: i32, y: i32, k: i32) -> bool {
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
/// chains) of the keyroot representative `(x_l, y_l)`.
///
/// A single `tree_dist(x_l, y_l, e_max)` call fills the `td` cells for *every*
/// node pair `(top_x, top_y)` on these left paths, and any of those cells may
/// later be read by an enclosing keyroot pair that needs it computed with budget
/// `e_budget(top_x, top_y, k)`. Provisioning the call with only the
/// representative's own `e_budget(x_l, y_l, k)` under-budgets those inner cells:
/// a needed subtree distance is left at infinity, and `ted_k` then returns `k+1`
/// when the true TED is `<= k` (a false negative). Taking the max over the left
/// paths fixes this while keeping the budget tight (always `<= k`, since every
/// `e_budget <= k`). Ports the `compute_e_max` branch of the C++
/// `touzet_kr_set_tree_index_impl.h`, which the source left commented out.
fn e_max_over_left_paths(t1: &TopDiffIndex, t2: &TopDiffIndex, x_l: i32, y_l: i32, k: i32) -> i32 {
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

/// Unit cost model. `del == ins == 1.0`; `ren(a,b) == 0.0` iff label ids match.
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

/// Holds the band matrices `td_` / `fd_` and a subproblem counter, mirroring the
/// per-call state of `TEDAlgorithmTouzet`.
pub struct TopDiffState {
    /// Subtree distances, indexed by `(x, y)` postorder ids.
    pub td: BandMatrix,
    /// Subforest distances.
    pub fd: BandMatrix,
    /// Number of inner DP cells touched (diagnostic; mirrors C++ counter).
    pub subproblem_counter: u64,
}

impl TopDiffState {
    /// Equivalent of `init_matrices(t1_size, k)`.
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

    /// Tree edit distance between subtrees rooted at postorder ids `x` (in `t1`)
    /// and `y` (in `t2`), given remaining error budget `e` and original `k`.
    /// Verbatim port of `TEDAlgorithmTouzet::tree_dist` (`ted_algorithm_touzet.h:163`).
    #[allow(clippy::needless_range_loop)]
    pub fn tree_dist(
        &mut self,
        t1: &TopDiffIndex,
        t2: &TopDiffIndex,
        x: i32,
        y: i32,
        k: i32,
        e: i32,
    ) -> f64 {
        let inf = f64::INFINITY;
        let x_size = t1.postl_to_size[x as usize];
        let y_size = t2.postl_to_size[y as usize];

        // Offsets to translate i and j to postorder ids.
        let x_off = x - x_size;
        let y_off = y - y_size;

        // Helpers to access matrices with i32 indices (BandMatrix takes usize but
        // its translation tolerates transient underflow via isize).
        // For fd_ rows/cols and td_ ids, indices are always >= 0 where written.

        // Initial cases.
        self.fd.set(0, 0, 0.0); // (0,0) always within e-strip.
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

        // General cases.
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

                let mut fd_read = inf;
                if i_forest != 0 || j_forest != 0 {
                    let mut td_read = inf;
                    if ((i + x_off) - (j + y_off)).abs() <= k {
                        td_read = self.td.read_at((i + x_off) as usize, (j + y_off) as usize);
                    }
                    if (0).max(i_forest - e - 1) <= j_forest
                        && j_forest <= (i_forest + e + 1).min(y_size)
                    {
                        fd_read = self.fd.read_at(i_forest as usize, j_forest as usize);
                    }
                    candidate_result = candidate_result.min(fd_read + td_read);
                } else {
                    // Pair of two subtrees.
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

/// Bounded tree edit distance via the Touzet KR-set algorithm. Returns the exact
/// TED when it is `<= k`, otherwise `k + 1` (the over-bound convention used by
/// the bounded lower-bound algorithms in this workspace).
///
/// Port of `TouzetKRSetTreeIndex::ted_k` (`touzet_kr_set_tree_index_impl.cpp:32`).
pub fn ted_k(t1: &TopDiffIndex, t2: &TopDiffIndex, k: i32) -> i32 {
    use rustc_hash::FxHashMap;

    let t1_size = t1.tree_size;
    let t2_size = t2.tree_size;

    let mut state = TopDiffState::new(t1_size, k);

    // Root pair outside the k-strip (size difference too large) -> infinity.
    if (t1_size - t2_size).abs() > k {
        return k + 1;
    }

    // Map packed keyroot pair -> index into kr_vector.
    let mut kr_pair_to_index: FxHashMap<u64, usize> = FxHashMap::default();
    // Collected (top_x, top_y) pairs.
    let mut kr_vector: Vec<(i32, i32)> = Vec::new();

    // Nested loop over node pairs in the k-strip, in decreasing postorder.
    for x in (0..t1_size).rev() {
        let x_keyroot = t1.postl_to_kr_ancestor[x as usize];
        let mut y = (x + k).min(t2_size - 1);
        let y_low = (0).max(x - k);
        while y >= y_low {
            if k_relevant(t1, t2, x, y, k) {
                let key = ((x_keyroot as u64) << 32) | (t2.postl_to_kr_ancestor[y as usize] as u64);
                match kr_pair_to_index.get(&key) {
                    None => {
                        kr_pair_to_index.insert(key, kr_vector.len());
                        kr_vector.push((x, y));
                    }
                    Some(&idx) => {
                        // Update top_y to the max of current y and stored top_y.
                        if y > kr_vector[idx].1 {
                            kr_vector[idx].1 = y;
                        }
                    }
                }
            }
            y -= 1;
        }
    }

    // Iterate collected pairs backwards and run forest distance.
    for &(x_l, y_l) in kr_vector.iter().rev() {
        // e_max must cover every inner (left-path) node pair this forest DP
        // computes, not just the representative; see `e_max_over_left_paths`.
        let e_max = e_max_over_left_paths(t1, t2, x_l, y_l, k);
        let d = state.tree_dist(t1, t2, x_l, y_l, k, e_max);
        state.td.set(x_l as usize, y_l as usize, d);
    }

    let result = state
        .td
        .read_at((t1_size - 1) as usize, (t2_size - 1) as usize);
    // Over-bound convention: > k or infinite -> k+1.
    if !result.is_finite() || result > k as f64 {
        return k + 1;
    }
    result as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_parsing::{parse_single, LabelDict};

    const INF: f64 = f64::INFINITY;

    /// Parse a bracket string against a shared label dict.
    fn pt(s: &str, dict: &mut LabelDict) -> ParsedTree {
        parse_single(s.to_string(), dict)
    }


    /// Build a TopDiffIndex from a bracket string with a fresh dict.
    fn build(s: &str) -> TopDiffIndex {
        let mut dict = LabelDict::default();
        TopDiffIndex::from_tree(&pt(s, &mut dict))
    }

    fn sorted(v: &[i32]) -> Vec<i32> {
        let mut v = v.to_vec();
        v.sort_unstable();
        v
    }

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
        // {a{b}{c}} -> postorder: b=0, c=1, a=2
        let idx = build("{a{b}{c}}");
        assert_eq!(idx.tree_size, 3);
        assert_eq!(idx.postl_to_size, vec![1, 1, 3]);
        assert_eq!(idx.postl_to_depth, vec![1, 1, 0]);
        assert_eq!(idx.postl_to_lch, vec![-1, -1, 0]);
        // keyroots: non-first child c=1, root a=2
        assert_eq!(sorted(&idx.list_kr), vec![1, 2]);
        assert_eq!(idx.postl_to_kr_ancestor, vec![2, 1, 2]);
    }

    #[test]
    fn from_tree_nested() {
        // {a{b{d}}{c}} -> postorder: d=0, b=1, c=2, a=3
        let idx = build("{a{b{d}}{c}}");
        assert_eq!(idx.tree_size, 4);
        assert_eq!(idx.postl_to_size, vec![1, 2, 1, 4]);
        assert_eq!(idx.postl_to_depth, vec![2, 1, 1, 0]);
        assert_eq!(idx.postl_to_lch, vec![-1, 0, -1, 1]);
        // keyroots: c=2 (non-first child of a), root a=3
        assert_eq!(sorted(&idx.list_kr), vec![2, 3]);
        assert_eq!(idx.postl_to_kr_ancestor, vec![3, 3, 2, 3]);
    }

    #[test]
    fn e_budget_and_k_relevant() {
        // t1 = {a{b}{c}}: size [1,1,3], depth [1,1,0]
        let t1 = build("{a{b}{c}}");
        // Root vs root, identical tree, k=5: lower bound 0, full budget.
        assert_eq!(e_budget(&t1, &t1, 2, 2, 5), 5);
        assert!(k_relevant(&t1, &t1, 2, 2, 5));

        // t2 = {a{b{d}}{c}}: size [1,2,1,4], depth [2,1,1,0]
        let t2 = build("{a{b{d}}{c}}");
        // x=0 (leaf b in t1: size 1, depth 1), y=3 (root in t2: size 4, depth 0), k=0.
        // term1 = |(3-1-1)-(4-4-0)| = |1-0| = 1
        // term2 = |1-0| = 1
        // term3 = |((1)-1)-((4)-4)| = 0
        // e_budget lower bound = 2 -> e_budget = 0 - 2 = -2 (stays negative).
        assert_eq!(e_budget(&t1, &t2, 0, 3, 0), -2);
        assert!(e_budget(&t1, &t2, 0, 3, 0) < 0);
        // k_relevant adds ||T1_x|-|T2_y|| = |1-4| = 3 -> lb = 4 > 0 -> false.
        assert!(!k_relevant(&t1, &t2, 0, 3, 0));
    }

    /// Run tree_dist on the root pair of two trees with a generous budget.
    fn tree_dist_roots(s1: &str, s2: &str, k: i32, e: i32) -> f64 {
        let mut dict = LabelDict::default();
        let t1 = TopDiffIndex::from_tree(&pt(s1, &mut dict));
        let t2 = TopDiffIndex::from_tree(&pt(s2, &mut dict));
        let mut state = TopDiffState::new(t1.tree_size, k);
        let x = t1.tree_size - 1;
        let y = t2.tree_size - 1;
        state.tree_dist(&t1, &t2, x, y, k, e)
    }

    #[test]
    fn tree_dist_identical_single_node() {
        assert_eq!(tree_dist_roots("{a}", "{a}", 4, 4), 0.0);
    }

    #[test]
    fn tree_dist_single_rename() {
        assert_eq!(tree_dist_roots("{a}", "{b}", 4, 4), 1.0);
    }

    #[test]
    fn tree_dist_one_delete() {
        // {a{b}} vs {a}: delete b -> distance 1.
        assert_eq!(tree_dist_roots("{a{b}}", "{a}", 4, 4), 1.0);
    }

    /// Parse both strings against ONE shared dict, build both indices, run ted_k.
    fn ted_pair(s1: &str, s2: &str, k: i32) -> i32 {
        let mut dict = LabelDict::default();
        let t1 = TopDiffIndex::from_tree(&pt(s1, &mut dict));
        let t2 = TopDiffIndex::from_tree(&pt(s2, &mut dict));
        ted_k(&t1, &t2, k)
    }

    #[test]
    fn ted_k_cases() {
        assert_eq!(ted_pair("{a{b}{c}}", "{a{b}{c}}", 5), 0);
        assert_eq!(ted_pair("{a{b}{c}}", "{a{b}{x}}", 5), 1);
        // size diff (1 vs 4) > k=1 -> k+1 = 2
        assert_eq!(ted_pair("{a}", "{a{b}{c}{d}}", 1), 2);
        // true TED 3 (rename a,b,c) but k=2 caps at k+1 = 3
        assert_eq!(ted_pair("{a{b}{c}}", "{x{y}{z}}", 2), 3);
        // exact with k=5
        assert_eq!(ted_pair("{a{b}{c}}", "{x{y}{z}}", 5), 3);
    }

    /// Deterministic LCG for seeded random tree generation.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            // Numerical Recipes constants.
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

    /// Build a random bracket-notation tree with up to `max_nodes` nodes from a
    /// small label alphabet.
    fn random_tree(rng: &mut Lcg, max_nodes: usize) -> String {
        let labels = ["a", "b", "c", "d"];
        let target = 1 + rng.range(max_nodes);
        let mut remaining = target - 1; // nodes left to attach beyond the root
        fn build(rng: &mut Lcg, labels: &[&str], remaining: &mut usize) -> String {
            let mut s = String::from("{");
            s.push_str(labels[rng.range(labels.len())]);
            // Attach a random number of children while budget remains.
            while *remaining > 0 {
                // ~50% chance to add a child at each step (decaying with budget).
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

    /// Postorder data needed by the Zhang-Shasha oracle: label ids, leftmost
    /// leaf descendants, and keyroots (all 0-based postorder ids).
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

        // Keyroots: nodes with no later node sharing the same leftmost leaf.
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

    /// Independent exact-TED oracle: textbook Zhang-Shasha with unit costs.
    /// Replaces the C++ FFI oracle (`tree_topdiff_bounded`) of the source repo.
    fn zhang_shasha(t1: &ZsInfo, t2: &ZsInfo) -> i32 {
        let (n, m) = (t1.labels.len(), t2.labels.len());
        let mut td = vec![vec![0i32; m]; n];

        // Keyroots are in increasing postorder, so inner td entries are
        // always filled before an outer keyroot pair needs them.
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

    /// Exact TED with the same over-bound convention as `ted_k`.
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

    #[test]
    fn differential_vs_zhang_shasha_oracle() {
        // Deterministic hand-written pairs covering the key categories.
        let mut pairs: Vec<(String, String)> = vec![
            // Identical.
            ("{a}".into(), "{a}".into()),
            ("{a{b}{c}}".into(), "{a{b}{c}}".into()),
            ("{a{b{d}}{c}}".into(), "{a{b{d}}{c}}".into()),
            // One-edit (rename / insert / delete).
            ("{a{b}{c}}".into(), "{a{b}{x}}".into()),
            ("{a{b}}".into(), "{a}".into()),
            ("{a}".into(), "{a{b}}".into()),
            ("{a{b}{c}}".into(), "{a{b}{c}{d}}".into()),
            // Size-mismatched.
            ("{a}".into(), "{a{b}{c}{d}}".into()),
            ("{a{b{c{d}}}}".into(), "{a}".into()),
            // Structurally different.
            ("{a{b}{c}}".into(), "{x{y}{z}}".into()),
            ("{a{b{c}}}".into(), "{a{b}{c}}".into()),
            ("{r{a}{b}{c}{d}}".into(), "{r{a{b{c{d}}}}}".into()),
            ("{a{b{c}}{d{e}}}".into(), "{a{b{c}{d}}{e}}".into()),
            // Regression: true TED == k at the boundary, where using only the
            // representative's e_budget (instead of the max over its left paths)
            // left an inner td cell at infinity and returned k+1. See
            // `e_max_over_left_paths`. These all need k in {2,3} to bite.
            ("{a{b}}".into(), "{a{c}{b{a}}}".into()),
            ("{c{b}{a{a}}}".into(), "{c{d{c}}}".into()),
            ("{b{b{b}{c{d}}}}".into(), "{b{c}}".into()),
        ];

        // Seeded random pairs for breadth. Deeper trees (was 8 nodes) are needed:
        // the left-path budget gap grows with depth, so the boundary bug only
        // shows up on deeper inputs.
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        for _ in 0..300 {
            let a = random_tree(&mut rng, 24);
            let b = random_tree(&mut rng, 24);
            pairs.push((a, b));
        }

        let ks = [1, 2, 3, 4, 5, 8, 50];
        let mut checked = 0usize;
        for (s1, s2) in &pairs {
            for &k in &ks {
                let mut dict = LabelDict::default();
                let t1 = TopDiffIndex::from_tree(&pt(s1, &mut dict));
                let t2 = TopDiffIndex::from_tree(&pt(s2, &mut dict));
                let got = ted_k(&t1, &t2, k);
                let want = oracle(s1, s2, k);
                assert_eq!(
                    got, want,
                    "mismatch for s1={s1} s2={s2} k={k}: rust={got} oracle={want}"
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 200,
            "expected a broad sweep, only checked {checked}"
        );
    }

    #[test]
    fn band_matrix_translate_and_fill() {
        let mut m = BandMatrix::new(5, 4, INF);
        // Untouched cell reads back the fill value.
        assert_eq!(m.read_at(2, 3), INF);
        // After a write, the same cell reads back the written value.
        m.set(2, 3, 7.0);
        assert_eq!(m.read_at(2, 3), 7.0);
        // The mutable accessor maps to the same backing cell.
        assert_eq!(*m.at(2, 3), 7.0);

        // band_width = 0 edge: single-column backing store, 1 row.
        let mut z = BandMatrix::new(1, 0, INF);
        assert_eq!(z.read_at(0, 0), INF);
        z.set(0, 0, 3.5);
        assert_eq!(z.read_at(0, 0), 3.5);
    }
}
