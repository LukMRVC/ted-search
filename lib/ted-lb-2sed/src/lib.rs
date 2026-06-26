use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod, TraversalKind, TraversalSelection};
use ted_lb_sed::traverse;
use tree_parsing::ParsedTree;

#[derive(Default)]
pub struct Sed2Algorithm {
    traversal_selection: TraversalSelection,
}

impl Sed2Algorithm {
    pub fn new(first: TraversalKind, second: TraversalKind) -> Self {
        Self {
            traversal_selection: TraversalSelection { first, second },
        }
    }
}

impl LowerBoundMethod for Sed2Algorithm {
    const NAME: &'static str = "2SED";
    const SUPPORTS_INDEX: bool = false;
    // The lower bound is the min-bottleneck of the combined `C_pre_sum` lattice
    // minus one (see `sed_2`); it directly bounds TED, so the search pipeline
    // compares it against 1 * threshold.
    const DIVISOR: usize = 1;

    type PreprocessedDataType = Sed2Index;
    type IndexType = ();
    type IndexParams = ();

    fn preprocess(&self, data: &[ParsedTree]) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data
            .iter()
            .map(|tree| preprocess_tree(tree, self.traversal_selection))
            .collect::<Vec<_>>())
    }

    fn lower_bound(
        &self,
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        sed_2(query, data, threshold)
    }

    fn build_index(
        &self,
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        Err("Indexing not supported for 2SED".to_string())
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

/// Preprocessed tree data for the 2SED lower bound.
///
/// In addition to the two traversal label sequences, this carries the
/// `second_to_first` lookup table: the permutation that maps a node's position
/// in the *second* traversal to its position in the *first* traversal. Because a
/// node sits at different indices in (e.g.) preorder vs postorder, there is no
/// O(1) arithmetic between the two positions — the permutation is the simplest
/// and fastest way to recover a node's first-traversal cell while walking the
/// second traversal's matrix.
#[derive(Debug, Clone)]
pub struct Sed2Index {
    pub first_traversal: Vec<i32>,
    pub second_traversal: Vec<i32>,
    /// `second_to_first[p]` = first-traversal position of the node that occupies
    /// second-traversal position `p`.
    pub second_to_first: Vec<usize>,
    pub tree_size: usize,
}

/// A fully materialized string-edit-distance DP matrix, retained so that any
/// individual cell `[i, j]` can be read back after the computation.
///
/// `get(i, j)` is the edit distance between the length-`i` prefix of the first
/// sequence and the length-`j` prefix of the second (so node at position `p`
/// influences matrix index `p + 1`).
#[derive(Debug, Clone)]
pub struct FullMatrix {
    /// `s1.len() + 1`
    pub rows: usize,
    /// `s2.len() + 1`
    pub cols: usize,
    data: Vec<usize>,
}

impl FullMatrix {
    #[inline]
    pub fn get(&self, i: usize, j: usize) -> usize {
        self.data[i * self.cols + j]
    }
}

/// Computes the full string-edit-distance DP matrix between `s1` and `s2`.
///
/// Unlike [`exact_string_edit_distance`], which keeps only a single rolling row,
/// this retains every cell so the 2SED lookup can read an arbitrary cell later.
pub fn full_string_edit_distance(s1: &[i32], s2: &[i32]) -> FullMatrix {
    use std::cmp::min;
    let rows = s1.len() + 1;
    let cols = s2.len() + 1;
    let mut data = vec![0usize; rows * cols];

    for j in 0..cols {
        data[j] = j;
    }
    for i in 1..rows {
        data[i * cols] = i;
        for j in 1..cols {
            let cost = usize::from(s1[i - 1] != s2[j - 1]);
            let del = data[(i - 1) * cols + j] + 1;
            let ins = data[i * cols + (j - 1)] + 1;
            let sub = data[(i - 1) * cols + (j - 1)] + cost;
            data[i * cols + j] = min(min(del, ins), sub);
        }
    }

    FullMatrix { rows, cols, data }
}

/// Inverts a permutation. Given `second_to_first` (second-traversal position ->
/// first-traversal position), returns `first_to_second` (first-traversal
/// position -> second-traversal position).
fn invert_permutation(second_to_first: &[usize]) -> Vec<usize> {
    let mut first_to_second = vec![0usize; second_to_first.len()];
    for (second_pos, &first_pos) in second_to_first.iter().enumerate() {
        first_to_second[first_pos] = second_pos;
    }
    first_to_second
}

/// The 2SED lower bound: the min-bottleneck value of the `C_pre_sum` lattice,
/// minus one.
///
/// `C_pre_sum[i][j]` sums the two single-traversal SED matrices for the node
/// pair aligned at first-traversal prefix lengths `i` and `j`: the first matrix
/// cell `C_pre[i][j]` plus the second matrix cell `C_post` for the *same* two
/// nodes (located via the `second_to_first` permutation, not the raw index).
///
/// A monotone path (right/down/diagonal) from `(0,0)` to `(n,m)` is an alignment
/// of the two trees' first traversals; [`min_bottleneck`] returns the cheapest
/// achievable "largest cell on the path". The final `-1` cancels the node the
/// bottleneck cut double-counts: that node lies in both the first-traversal
/// prefix and the second-traversal prefix, so its edit is paid in both matrices.
pub fn sed_2(query: &Sed2Index, data: &Sed2Index, _threshold: usize) -> usize {
    let c_pre = full_string_edit_distance(&query.first_traversal, &data.first_traversal);
    let c_post = full_string_edit_distance(&query.second_traversal, &data.second_traversal);
    min_bottleneck(query, data, &c_pre, &c_post).saturating_sub(1)
}

/// Minimum-bottleneck value of the implicit `C_pre_sum` lattice: over every
/// monotone path from `(0,0)` to `(n,m)` (steps: right, down, diagonal), the
/// smallest achievable maximum cell value.
///
/// `C_pre_sum` is never materialized; each cell is read on demand via [`cell`].
/// The DP runs in row-major (topological) order, which is correct for this DAG
/// with diagonal edges — `dp[i][j]` is the best bottleneck of any path reaching
/// `(i, j)`, i.e. the max of the cheapest incoming path and the cell itself.
pub fn min_bottleneck(
    query: &Sed2Index,
    data: &Sed2Index,
    c_pre: &FullMatrix,
    c_post: &FullMatrix,
) -> usize {
    let n = query.first_traversal.len();
    let m = data.first_traversal.len();
    let first_to_second_q = invert_permutation(&query.second_to_first);
    let first_to_second_d = invert_permutation(&data.second_to_first);

    // C_pre_sum cell at first-traversal prefix lengths (i, j). The C_post term is
    // read at the second-traversal positions of the *same* two nodes (+1 because
    // row/column 0 is the empty prefix), so the matrices are summed per node. On
    // the empty-prefix boundary (i == 0 or j == 0) there is no node to map, so
    // the positional cell is used directly.
    let cell = |i: usize, j: usize| -> usize {
        let post = if i == 0 || j == 0 {
            c_post.get(i, j)
        } else {
            c_post.get(first_to_second_q[i - 1] + 1, first_to_second_d[j - 1] + 1)
        };
        c_pre.get(i, j) + post
    };

    let rows = n + 1;
    let cols = m + 1;
    let mut dp = vec![usize::MAX; rows * cols];
    dp[0] = cell(0, 0);
    for i in 0..rows {
        for j in 0..cols {
            if i == 0 && j == 0 {
                continue;
            }
            let mut best = usize::MAX;
            if i > 0 {
                best = best.min(dp[(i - 1) * cols + j]);
            }
            if j > 0 {
                best = best.min(dp[i * cols + (j - 1)]);
            }
            if i > 0 && j > 0 {
                best = best.min(dp[(i - 1) * cols + (j - 1)]);
            }
            dp[i * cols + j] = best.max(cell(i, j));
        }
    }
    dp[(rows - 1) * cols + (cols - 1)]
}

pub fn exact_string_edit_distance(s1: &[i32], s2: &[i32]) -> usize {
    use std::cmp::min;
    // assumes size of s2 is smaller or equal than s1
    let s2len = s2.len();
    let mut cache: Vec<usize> = (1..s2len + 1).collect();
    let mut result = s2len;
    for (i, ca) in s1.iter().enumerate() {
        let mut dist_b = i;
        result = i + 1;

        for (j, cb) in s2.iter().enumerate() {
            let dist_a = dist_b + usize::from(ca != cb);
            unsafe {
                dist_b = *cache.get_unchecked(j);
                result = min(result + 1, min(dist_a, dist_b + 1));
                *cache.get_unchecked_mut(j) = result;
            }
        }
    }

    result
}

fn preprocess_tree(
    tree: &ParsedTree,
    selection: TraversalSelection,
) -> <Sed2Algorithm as LowerBoundMethod>::PreprocessedDataType {
    let Some(root) = tree.iter().next() else {
        panic!("Unable to get root but tree is not empty!");
    };

    let root_id = tree.get_node_id(root).expect("Failed to get root node id");

    // Label sequences for the two selected traversals.
    let mut pre = Vec::new();
    let mut post = Vec::new();
    let mut reversed_preorder = Vec::new();
    let mut reversed_postorder = Vec::new();

    let mut reserve_memory = |kind: TraversalKind| match kind {
        TraversalKind::Preorder => pre.reserve(tree.count()),
        TraversalKind::Postorder => post.reserve(tree.count()),
        TraversalKind::ReversedPreorder => reversed_preorder.reserve(tree.count()),
        TraversalKind::ReversedPostorder => reversed_postorder.reserve(tree.count()),
    };

    reserve_memory(selection.first);
    reserve_memory(selection.second);

    traverse(
        root_id,
        tree,
        selection,
        &mut pre,
        &mut post,
        &mut reversed_preorder,
        &mut reversed_postorder,
    );

    reversed_preorder.reverse();
    reversed_postorder.reverse();

    // Node-identity sequences for the same traversals, used to build the lookup
    // table. Each node's canonical id is its preorder rank, so it identifies the
    // same node across both traversal orders.
    let mut pre_ids = Vec::new();
    let mut post_ids = Vec::new();
    let mut reversed_pre_ids = Vec::new();
    let mut reversed_post_ids = Vec::new();
    let mut counter = 0usize;
    traverse_ids(
        root_id,
        tree,
        &mut counter,
        &mut pre_ids,
        &mut post_ids,
        &mut reversed_pre_ids,
        &mut reversed_post_ids,
    );
    reversed_pre_ids.reverse();
    reversed_post_ids.reverse();

    let order_ids = |kind: TraversalKind| -> &Vec<usize> {
        match kind {
            TraversalKind::Preorder => &pre_ids,
            TraversalKind::Postorder => &post_ids,
            TraversalKind::ReversedPreorder => &reversed_pre_ids,
            TraversalKind::ReversedPostorder => &reversed_post_ids,
        }
    };

    let first_ids = order_ids(selection.first);
    let second_ids = order_ids(selection.second);

    let mut pos_in_first = vec![0usize; tree.count()];
    for (pos, &node) in first_ids.iter().enumerate() {
        pos_in_first[node] = pos;
    }
    let second_to_first: Vec<usize> = second_ids.iter().map(|&node| pos_in_first[node]).collect();

    Sed2Index {
        first_traversal: match selection.first {
            TraversalKind::Preorder => pre.clone(),
            TraversalKind::Postorder => post.clone(),
            TraversalKind::ReversedPreorder => reversed_preorder.clone(),
            TraversalKind::ReversedPostorder => reversed_postorder.clone(),
        },
        second_traversal: match selection.second {
            TraversalKind::Preorder => pre.clone(),
            TraversalKind::Postorder => post.clone(),
            TraversalKind::ReversedPreorder => reversed_preorder.clone(),
            TraversalKind::ReversedPostorder => reversed_postorder.clone(),
        },
        second_to_first,
        tree_size: tree.count(),
    }
}

/// Records node identities in the four traversal orders, mirroring the push
/// pattern of [`ted_lb_sed::traverse`] so position `i` of an id sequence refers
/// to the same node as position `i` of the corresponding label sequence.
///
/// A node's id is its preorder rank (`counter`), which is stable across both
/// traversals.
fn traverse_ids(
    nid: NodeId,
    tree: &ParsedTree,
    counter: &mut usize,
    pre: &mut Vec<usize>,
    post: &mut Vec<usize>,
    reversed_pre: &mut Vec<usize>,
    reversed_post: &mut Vec<usize>,
) {
    let id = *counter;
    *counter += 1;
    pre.push(id);
    reversed_post.push(id);
    for cnid in nid.children(tree) {
        traverse_ids(cnid, tree, counter, pre, post, reversed_pre, reversed_post);
    }
    reversed_pre.push(id);
    post.push(id);
}

pub struct Sed2Factory;

impl AlgorithmFactory for Sed2Factory {
    type AlgorithmType = Sed2Algorithm;

    fn create_algorithm() -> Self::AlgorithmType {
        Sed2Algorithm::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_parsing::{parse_single, LabelDict};

    #[test]
    fn test_exact_string_edit_distance() {
        let kitten = "kitten".chars().map(|c| c as i32).collect::<Vec<_>>();
        let sitting = "sitting".chars().map(|c| c as i32).collect::<Vec<_>>();
        assert_eq!(exact_string_edit_distance(&kitten, &sitting), 3);
    }

    #[test]
    fn test_full_matrix_corner_and_interior() {
        let kitten = "kitten".chars().map(|c| c as i32).collect::<Vec<_>>();
        let sitting = "sitting".chars().map(|c| c as i32).collect::<Vec<_>>();
        let m = full_string_edit_distance(&kitten, &sitting);
        // Corner equals the full edit distance.
        assert_eq!(m.get(kitten.len(), sitting.len()), 3);
        // Interior: prefix "k" vs "si" needs 2 edits.
        assert_eq!(m.get(1, 2), 2);
    }

    #[test]
    fn test_second_to_first_permutation() {
        // Tree:  a -> (b, c)
        //   preorder labels:  a, b, c   (node ids 0, 1, 2)
        //   postorder labels: b, c, a   (node ids 1, 2, 0)
        // second(postorder) positions map to first(preorder) positions [1, 2, 0].
        let mut ld = LabelDict::default();
        let tree = parse_single("{a{b}{c}}".to_string(), &mut ld);

        let algo = Sed2Algorithm::default(); // first = preorder, second = postorder
        let idx = algo
            .preprocess(std::slice::from_ref(&tree))
            .unwrap()
            .remove(0);

        assert_eq!(idx.second_to_first, vec![1, 2, 0]);
    }

    #[test]
    fn test_sed_2_on_indices() {
        let t1 = Sed2Index {
            first_traversal: vec![1, 2, 3],
            second_traversal: vec![2, 3, 1],
            second_to_first: vec![0, 1, 2],
            tree_size: 3,
        };
        let t2 = Sed2Index {
            first_traversal: vec![1, 3, 4],
            second_traversal: vec![3, 4, 1],
            second_to_first: vec![0, 1, 2],
            tree_size: 3,
        };

        // Identity permutations, so C_pre_sum is the elementwise sum of the two
        // matrices:
        //     0 2 4 6
        //     2 1 3 5
        //     4 2 3 5
        //     6 4 3 4
        // The cheapest monotone path (the diagonal: 0, 1, 3, 4) has bottleneck 4,
        // and every path ends at the corner cell 4, so min_bottleneck = 4.
        // Final = min_bottleneck - 1 = 3.
        assert_eq!(sed_2(&t1, &t2, 100), 3);
    }

    #[test]
    fn verify_two_tree_example() {
        // Hand-worked example (preorder + reversed-postorder traversals).
        //
        //   T1: f -> x -> (k, u, h)        T2: x -> h -> (o -> m, r)
        //
        // reversed-postorder here = reverse(preorder), since `reversed_post` is
        // pushed at node entry and then reversed.
        let mut ld = LabelDict::default();
        let t1 = parse_single("{f{x{k}{u}{h}}}".to_string(), &mut ld);
        let t2 = parse_single("{x{h{o{m}}{r}}}".to_string(), &mut ld);

        let algo = Sed2Algorithm::new(TraversalKind::Preorder, TraversalKind::ReversedPostorder);
        let mut idx = algo.preprocess(&[t1, t2]).unwrap();
        let d2 = idx.remove(1);
        let q1 = idx.remove(0);

        // reversed-postorder is the exact reverse of preorder for a 5-node tree.
        assert_eq!(q1.second_to_first, vec![4, 3, 2, 1, 0]);
        assert_eq!(d2.second_to_first, vec![4, 3, 2, 1, 0]);

        // C_pre: full SED matrix of the preorder label sequences.
        let m_pre = full_string_edit_distance(&q1.first_traversal, &d2.first_traversal);
        #[rustfmt::skip]
        let expected_pre = [
            [0, 1, 2, 3, 4, 5],
            [1, 1, 2, 3, 4, 5],
            [2, 1, 2, 3, 4, 5],
            [3, 2, 2, 3, 4, 5],
            [4, 3, 3, 3, 4, 5],
            [5, 4, 3, 4, 4, 5],
        ];
        for (i, row) in expected_pre.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                assert_eq!(m_pre.get(i, j), v, "C_pre[{i},{j}]");
            }
        }

        // C_post: full SED matrix of the reversed-postorder label sequences.
        let m_post = full_string_edit_distance(&q1.second_traversal, &d2.second_traversal);
        #[rustfmt::skip]
        let expected_post = [
            [0, 1, 2, 3, 4, 5],
            [1, 1, 2, 3, 3, 4],
            [2, 2, 2, 3, 4, 4],
            [3, 3, 3, 3, 4, 5],
            [4, 4, 4, 4, 4, 4],
            [5, 5, 5, 5, 5, 5],
        ];
        for (i, row) in expected_post.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                assert_eq!(m_post.get(i, j), v, "C_post[{i},{j}]");
            }
        }

        // min_bottleneck over C_pre_sum is 6 (diagonal of all-6 cells); the lower
        // bound is min_bottleneck - 1 = 5. Exact TED for this pair is 6, so 5 is a
        // valid (non-exceeding) lower bound.
        assert_eq!(sed_2(&q1, &d2, 100), 5);
    }

    #[test]
    fn verify_two_tree_example_b() {
        // Second hand-worked pair, which previously exposed an over-estimate.
        //
        //   T1: y -> (c, a -> d)        T2: k -> (c, d -> (w, i))
        //
        // min_bottleneck over C_pre_sum is 5; the lower bound is 5 - 1 = 4, which
        // exactly matches the true TED of 4 (a relabel y->k, a->d, d->w and an
        // insert of i). Without the -1 the bound would be 5 > TED, i.e. invalid.
        let mut ld = LabelDict::default();
        let t1 = parse_single("{y{c}{a{d}}}".to_string(), &mut ld);
        let t2 = parse_single("{k{c}{d{w}{i}}}".to_string(), &mut ld);

        let algo = Sed2Algorithm::new(TraversalKind::Preorder, TraversalKind::ReversedPostorder);
        let mut idx = algo.preprocess(&[t1, t2]).unwrap();
        let d2 = idx.remove(1);
        let q1 = idx.remove(0);

        assert_eq!(sed_2(&q1, &d2, 100), 4);
        // The lower bound is symmetric in its two arguments.
        assert_eq!(sed_2(&d2, &q1, 100), 4);
    }
}
