use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod, TraversalKind, TraversalSelection};
use tree_parsing::{LabelId, ParsedTree};

use crate::index_gram::IndexGram;

pub(crate) type Traversal = Vec<LabelId>;

mod index_gram;

#[derive(Debug, Clone)]
/// A struct representing the String edit distance (SED) index.
///
/// The SED index is a data structure used for efficient querying of tree-structured data.
/// It stores the preorder and postorder traversal sequences of the tree, as well as the tree size.
///
pub struct SEDIndex {
    pub first_traversal: Traversal,
    pub second_traversal: Traversal,
    pub tree_size: usize,
}

/// Specific parameters for the SED index construction.
#[derive(Debug, Clone)]
pub struct IndexParams {
    // q-gram size
    pub q: usize,
}

pub struct SedAlgorithm {
    traversal_selection: TraversalSelection,
}

impl SedAlgorithm {
    pub fn new(first: TraversalKind, second: TraversalKind) -> Self {
        Self {
            traversal_selection: TraversalSelection { first, second },
        }
    }

    pub fn with_selection(traversal_selection: TraversalSelection) -> Self {
        Self {
            traversal_selection,
        }
    }
}

impl Default for SedAlgorithm {
    fn default() -> Self {
        Self {
            traversal_selection: TraversalSelection::default(),
        }
    }
}

impl LowerBoundMethod for SedAlgorithm {
    const NAME: &'static str = "SED";
    // TODO: Add QGram Index support
    const SUPPORTS_INDEX: bool = true;

    type PreprocessedDataType = SEDIndex;
    type IndexType = IndexGram;
    type IndexParams = IndexParams;

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
        sed_k(query, data, threshold)
    }

    fn build_index(
        &self,
        data: &[Self::PreprocessedDataType],
        params: &IndexParams,
    ) -> Result<Self::IndexType, String> {
        let preorder = data
            .iter()
            .cloned()
            .map(|si| si.first_traversal)
            .collect::<Vec<_>>();
        Ok(IndexGram::new(&preorder, params.q))
    }

    /// Query the index with the preprocessed query data
    /// and return a list of candidate indices
    ///
    /// The query must be cloned because it will be modified when querying the index.
    fn query_index(
        &self,
        query: &Self::PreprocessedDataType,
        index: &Self::IndexType,
        threshold: usize,
    ) -> Vec<usize> {
        index
            .query(query.first_traversal.clone(), threshold)
            .unwrap()
    }
}

/// Computes bounded string edit distance with known maximal threshold.
/// Returns distance at max of K. Algorithm by Hal Berghel and David Roach
#[inline]
fn sed_k(t1: &SEDIndex, t2: &SEDIndex, k: usize) -> usize {
    let (mut t1, mut t2) = (t1, t2);
    if t1.tree_size.abs_diff(t2.tree_size) > k {
        return k + 1;
    }

    // if size of t1 is bigger than t2, swap them
    if t1.tree_size > t2.tree_size {
        (t1, t2) = (t2, t1);
    }

    let first_dist = bounded_string_edit_distance(&t1.first_traversal, &t2.first_traversal, k);

    if first_dist > k {
        return first_dist;
    }

    let second_dist = bounded_string_edit_distance(&t1.second_traversal, &t2.second_traversal, k);

    std::cmp::max(first_dist, second_dist)
}

/// Computes the bounded string edit distance between two sequences with a given threshold k.
/// This is an implementation of the algorithm by Hal Berghel and David Roach.
fn bounded_string_edit_distance(s1: &[i32], s2: &[i32], k: usize) -> usize {
    // assumes size of s2 is bigger or equal than s1
    use std::cmp::{max, min};
    let (s1, s2) = if s1.len() <= s2.len() {
        (s1, s2)
    } else {
        (s2, s1)
    };
    let s1len = s1.len() as i64;
    let s2len = s2.len() as i64;

    let threshold = min(s2len, k as i64);
    let size_diff = s2len - s1len;
    if size_diff > threshold {
        return usize::MAX;
    }

    let zero_k: i64 = ((if s1len < threshold { s1len } else { threshold }) >> 1) + 2;

    let arr_len = size_diff + (zero_k) * 2 + 2;

    let condition_diag = size_diff + zero_k;
    let end_max = (condition_diag) << 1;

    let mut current_row = vec![-1i64; arr_len as usize];
    let mut next_row = vec![-1i64; arr_len as usize];

    for i in 1..=threshold + 1 {
        std::mem::swap(&mut next_row, &mut current_row);

        // Calculate original band boundaries from Berghel-Roach algorithm
        let original_start: i64;
        if i <= zero_k {
            original_start = -i + 1;
        } else {
            original_start = i - (zero_k << 1) + 1;
        }

        let original_end: i64;
        if i <= condition_diag {
            original_end = i;
            unsafe {
                *next_row.get_unchecked_mut((zero_k + i) as usize) = -1;
            }
        } else {
            original_end = end_max - i;
        }

        // Precompute valid diagonal range based on budget
        let budget = threshold - (i - 1);

        // If budget is negative or zero, only the target diagonal is valid
        let (min_valid_diag, max_valid_diag) = if budget <= 0 {
            (size_diff, size_diff)
        } else {
            (size_diff - budget, size_diff + budget)
        };

        // Intersect the original band with the budget-constrained range
        let start = max(original_start, min_valid_diag);
        let end = min(original_end, max_valid_diag + 1); // +1 because range is exclusive

        // Initialize cell variables for the adjusted starting position
        // These represent values from the previous cost level (i-1):
        // - current_cell: value at diagonal (start - 1)
        // - next_cell: value at diagonal (start)
        let mut current_cell: i64;
        let mut next_cell: i64;
        let mut previous_cell: i64;

        // Load initial values from previous row based on adjusted start position
        if i <= zero_k && start == original_start {
            // Original initialization for the standard case
            current_cell = -1;
            next_cell = i - 2i64;
        } else {
            // When start is adjusted, load values from the appropriate positions
            unsafe {
                let start_idx = (zero_k + start) as usize;
                current_cell = if start > original_start && start_idx > 0 {
                    *current_row.get_unchecked(start_idx - 1)
                } else {
                    -1
                };
                next_cell = *current_row.get_unchecked(start_idx);
            }
        }

        let mut row_index = (start + zero_k) as usize - 1;

        let mut t;

        for q in start..end {
            row_index += 1;
            previous_cell = current_cell;
            current_cell = next_cell;
            unsafe {
                next_cell = *current_row.get_unchecked(row_index + 1);
            }

            // max()
            t = max(max(current_cell + 1, previous_cell), next_cell + 1);

            unsafe {
                while t < s1len
                    && (t + q) < s2len
                    && s1.get_unchecked(t as usize) == s2.get_unchecked((t + q) as usize)
                {
                    t += 1;
                }
            }

            unsafe {
                *next_row.get_unchecked_mut(row_index) = t;
            }
        }

        unsafe {
            let condition_value = *next_row.get_unchecked(condition_diag as usize);
            if condition_value >= s1len {
                return (i - 1) as usize;
            }
        }
    }

    usize::MAX
}

/// Gets the SED index for a given tree
fn preprocess_tree(
    tree: &ParsedTree,
    selection: TraversalSelection,
) -> <SedAlgorithm as LowerBoundMethod>::PreprocessedDataType {
    let Some(root) = tree.iter().next() else {
        panic!("Unable to get root but tree is not empty!");
    };

    let root_id = tree.get_node_id(root).expect("Failed to get root node id");

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

    SEDIndex {
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
        tree_size: tree.count(),
    }
}

/// Traverses the tree, recording the preorder and postorder sequences  of labels.
pub fn traverse(
    nid: NodeId,
    tree: &ParsedTree,
    selection: TraversalSelection,
    pre: &mut Vec<i32>,
    post: &mut Vec<i32>,
    reversed_pre: &mut Vec<i32>,
    reversed_post: &mut Vec<i32>,
) {
    // i am here at the current root
    // Retrieves the label associated with a given node ID from the tree.
    let label = tree.get(nid).unwrap().get();
    pre.push(*label);
    reversed_post.push(*label);
    for cnid in nid.children(tree) {
        traverse(
            cnid,
            tree,
            selection,
            pre,
            post,
            reversed_pre,
            reversed_post,
        );
    }
    reversed_pre.push(*label);
    post.push(*label);
}

pub struct SedFactory;

impl AlgorithmFactory for SedFactory {
    type AlgorithmType = SedAlgorithm;
    fn create_algorithm() -> Self::AlgorithmType {
        SedAlgorithm::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sed_k() {
        let v1 = vec![1, 2, 3, 4, 5, 5, 6];
        let v2 = vec![1, 2, 3, 5, 6, 7, 6];

        let result = bounded_string_edit_distance(&v1, &v2, 2);
        assert_eq!(result, usize::MAX);

        let result = bounded_string_edit_distance(&v1, &v2, 4);
        assert_eq!(result, 3);
    }

    #[test]
    fn test_sed_br_impl() {
        let query = "aaa".chars().map(|c| c as i32).collect::<Vec<_>>();
        let target = "aaabcd".chars().map(|c| c as i32).collect::<Vec<_>>();

        let result = bounded_string_edit_distance(&query, &target, 3);
        assert_eq!(
            result, 3,
            "Expected edit distance of 3 between 'aaa' and 'aaabcd' with k=3"
        );

        let query = "garvey".chars().map(|c| c as i32).collect::<Vec<_>>();
        let target = "avery".chars().map(|c| c as i32).collect::<Vec<_>>();

        let result = bounded_string_edit_distance(&query, &target, 3);
        assert_eq!(
            result, 3,
            "Expected edit distance of 3 between 'garvey' and 'avery' with k=3"
        );

        let result = bounded_string_edit_distance(&query, &target, 2);
        assert_eq!(
            result,
            usize::MAX,
            "Expected edit non computable (distance > k) between 'garvey' and 'avery' with k=2"
        );

        let query = "abcde".chars().map(|c| c as i32).collect::<Vec<_>>();
        let target = "fghij".chars().map(|c| c as i32).collect::<Vec<_>>();

        let result = bounded_string_edit_distance(&query, &target, 5);
        assert_eq!(
            result, 5,
            "Expected edit distance of 5 between 'abcde' and 'fghij' with k=5"
        );

        let query = "kitten".chars().map(|c| c as i32).collect::<Vec<_>>();
        let target = "sitting".chars().map(|c| c as i32).collect::<Vec<_>>();

        let result = bounded_string_edit_distance(&query, &target, 3);
        assert_eq!(
            result, 3,
            "Expected edit distance of 3 between 'kitten' and 'sitting' with k=5"
        );

        let query = "123456452abc".chars().map(|c| c as i32).collect::<Vec<_>>();
        let target = "173829526452abc"
            .chars()
            .map(|c| c as i32)
            .collect::<Vec<_>>();

        let result = bounded_string_edit_distance(&query, &target, 4);
        assert_eq!(
            result,
            usize::MAX,
            "Expected edit distance of 3 between 'kitten' and 'sitting' with k=5"
        );

        // assert_eq!(matrix, initialized_fkp_target);
    }
}
