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
    // The lower bound sums two single-traversal SEDs, each of which bounds TED,
    // so the sum bounds 1 * TED. The search pipeline must therefore compare it
    // against 1 * threshold.
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

/// Reads the first-traversal matrix cell for the node pair that sits at the
/// given positions in the **second** traversal.
///
/// `query_pos2` / `data_pos2` are positions in the query's and data tree's
/// second traversals. The `second_to_first` lookup tables translate those into
/// the corresponding first-traversal positions, and the matrix is indexed at
/// `+1` because row/column 0 is the empty-prefix row/column.
#[inline]
pub fn first_matrix_cell(
    m1: &FullMatrix,
    query: &Sed2Index,
    data: &Sed2Index,
    query_pos2: usize,
    data_pos2: usize,
) -> usize {
    let i1 = query.second_to_first[query_pos2];
    let j1 = data.second_to_first[data_pos2];
    m1.get(i1 + 1, j1 + 1)
}

pub fn sed_2(query: &Sed2Index, data: &Sed2Index, threshold: usize) -> usize {
    // Retain the full first-traversal matrix so individual cells stay readable
    // (via `first_matrix_cell`) when combining node-aligned values.
    let m1 = full_string_edit_distance(&query.first_traversal, &data.first_traversal);
    let first_dist = m1.get(query.first_traversal.len(), data.first_traversal.len());

    // If the first traversal alone already exceeds the scaled cutoff, the sum can
    // only be larger, so prune early without computing the second traversal.
    if first_dist > threshold * Sed2Algorithm::DIVISOR {
        return first_dist;
    }
    let second_dist = exact_string_edit_distance(&query.second_traversal, &data.second_traversal);
    first_dist + second_dist
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
    fn test_first_matrix_cell_lookup() {
        // Hand-built indices: the node at second-traversal position 0 sits at
        // first-traversal position 1 for both trees, so the lookup must read the
        // first matrix at (1+1, 1+1) = (2, 2).
        let q = Sed2Index {
            first_traversal: vec![1, 2, 3],
            second_traversal: vec![2, 3, 1],
            second_to_first: vec![1, 2, 0],
            tree_size: 3,
        };
        let d = Sed2Index {
            first_traversal: vec![1, 3, 4],
            second_traversal: vec![3, 4, 1],
            second_to_first: vec![1, 2, 0],
            tree_size: 3,
        };
        let m1 = full_string_edit_distance(&q.first_traversal, &d.first_traversal);

        assert_eq!(first_matrix_cell(&m1, &q, &d, 0, 0), m1.get(2, 2));
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

        // first traversal SED = 2, second traversal SED = 2; 2SED sums them.
        // threshold * DIVISOR = 2 * 1 = 2, and first_dist (2) is not > 2,
        // so the second traversal is computed and added.
        assert_eq!(sed_2(&t1, &t2, 2), 4);
    }

    #[test]
    fn test_sed_2_early_return() {
        // first traversal SED alone exceeds threshold * DIVISOR (= 1 * 1 = 1),
        // so the second traversal is never computed and first_dist is returned.
        let t1 = Sed2Index {
            first_traversal: vec![1, 2, 3, 4],
            second_traversal: vec![0, 0, 0, 0],
            second_to_first: vec![0, 1, 2, 3],
            tree_size: 4,
        };
        let t2 = Sed2Index {
            first_traversal: vec![5, 6, 7, 8],
            second_traversal: vec![9, 9, 9, 9],
            second_to_first: vec![0, 1, 2, 3],
            tree_size: 4,
        };

        assert_eq!(sed_2(&t1, &t2, 1), 4);
    }
}
