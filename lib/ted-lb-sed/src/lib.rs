use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use tree_parsing::{LabelId, ParsedTree};

use crate::index_gram::IndexGram;

mod index_gram;

pub(crate) type Traversal = Vec<LabelId>;

#[derive(Debug, Clone)]
/// A struct representing the String edit distance (SED) index.
///
/// The SED index is a data structure used for efficient querying of tree-structured data.
/// It stores the preorder and postorder traversal sequences of the tree, as well as the tree size.
///
pub struct SEDIndex {
    pub preorder: Traversal,
    pub postorder: Traversal,
    pub tree_size: usize,
}

/// Specific parameters for the SED index construction.
#[derive(Debug, Clone)]
pub struct IndexParams {
    // q-gram size
    pub q: usize,
}

pub struct SedAlgorithm;

impl LowerBoundMethod for SedAlgorithm {
    const NAME: &'static str = "SED";
    // TODO: Add QGram Index support
    const SUPPORTS_INDEX: bool = true;

    type PreprocessedDataType = SEDIndex;
    type IndexType = IndexGram;
    type IndexParams = IndexParams;

    fn preprocess(&self, data: &[ParsedTree]) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data.iter().map(preprocess_tree).collect::<Vec<_>>())
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
            .map(|si| si.preorder)
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
        index.query(query.preorder.clone(), threshold).unwrap()
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
    if t1.preorder.len() > t2.preorder.len() {
        (t1, t2) = (t2, t1);
    }
    let post_dist = bounded_string_edit_distance(&t1.postorder, &t2.postorder, k);

    if post_dist > k {
        return post_dist;
    }
    let pre_dist = bounded_string_edit_distance(&t1.preorder, &t2.preorder, k);
    std::cmp::max(pre_dist, post_dist)
}

/// Computes the bounded string edit distance between two sequences with a given threshold k.
/// This is an implementation of the algorithm by Hal Berghel and David Roach.
fn bounded_string_edit_distance(s1: &[i32], s2: &[i32], k: usize) -> usize {
    // assumes size of s2 is bigger or equal than s1
    use std::cmp::{max, min};
    let s1len = s1.len() as i64;
    let s2len = s2.len() as i64;

    let threshold = min(s2len, k as i64);
    let size_diff = s2len - s1len;

    if threshold < size_diff {
        return threshold as usize;
    }

    let zero_k: i64 = ((if s1len < threshold { s1len } else { threshold }) >> 1) + 2;

    let arr_len = size_diff + (zero_k) * 2 + 2;

    let mut current_row = vec![-1i64; arr_len as usize];
    let mut next_row = vec![-1i64; arr_len as usize];
    let mut i = 0;
    let condition_row = size_diff + zero_k;
    let end_max = condition_row << 1;

    loop {
        i += 1;
        std::mem::swap(&mut next_row, &mut current_row);

        let start: i64;
        let mut next_cell: i64;
        let mut previous_cell: i64;
        let mut current_cell: i64 = -1;

        if i <= zero_k {
            start = -i + 1;
            next_cell = i - 2i64;
        } else {
            start = i - (zero_k << 1) + 1;
            unsafe {
                next_cell = *current_row.get_unchecked((zero_k + start) as usize);
            }
        }

        let end: i64;
        if i <= condition_row {
            end = i;
            unsafe {
                *next_row.get_unchecked_mut((zero_k + i) as usize) = -1;
            }
        } else {
            end = end_max - i;
        }

        let mut row_index = (start + zero_k) as usize;

        let mut t;

        for q in start..end {
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
            row_index += 1;
        }

        unsafe {
            if !(*next_row.get_unchecked(condition_row as usize) < s1len && i <= threshold) {
                if (*next_row.get_unchecked(condition_row as usize) < s1len) && i > threshold {
                    break usize::MAX;
                }
                break (i - 1) as usize;
            }
        }
    }
}

/// Gets the SED index for a given tree
fn preprocess_tree(tree: &ParsedTree) -> <SedAlgorithm as LowerBoundMethod>::PreprocessedDataType {
    let Some(root) = tree.iter().next() else {
        panic!("Unable to get root but tree is not empty!");
    };

    let root_id = tree.get_node_id(root).expect("Failed to get root node id");

    let mut pre = Vec::with_capacity(tree.count());
    let mut post = Vec::with_capacity(tree.count());

    traverse(root_id, tree, &mut pre, &mut post);

    SEDIndex {
        postorder: post,
        preorder: pre,
        tree_size: tree.count(),
    }
}

/// Traverses the tree, recording the preorder and postorder sequences  of labels.
fn traverse(nid: NodeId, tree: &ParsedTree, pre: &mut Vec<i32>, post: &mut Vec<i32>) {
    // i am here at the current root
    // Retrieves the label associated with a given node ID from the tree.
    let label = tree.get(nid).unwrap().get();
    pre.push(*label);
    for cnid in nid.children(tree) {
        traverse(cnid, tree, pre, post);
    }
    post.push(*label);
}

pub struct SedFactory;

impl AlgorithmFactory for SedFactory {
    type AlgorithmType = SedAlgorithm;
    fn create_algorithm() -> Self::AlgorithmType {
        SedAlgorithm
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
}
