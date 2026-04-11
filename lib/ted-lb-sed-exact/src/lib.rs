use ted_base::{AlgorithmFactory, LowerBoundMethod, TraversalKind, TraversalSelection};
use ted_lb_sed::{traverse, SEDIndex};
use tree_parsing::ParsedTree;

#[derive(Default)]
pub struct SedExactAlgorithm {
    traversal_selection: TraversalSelection,
}

impl SedExactAlgorithm {
    pub fn new(first: TraversalKind, second: TraversalKind) -> Self {
        Self {
            traversal_selection: TraversalSelection { first, second },
        }
    }
}


impl LowerBoundMethod for SedExactAlgorithm {
    const NAME: &'static str = "SED-EXACT";
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = SEDIndex;
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
        sed_exact(query, data, threshold)
    }

    fn build_index(
        &self,
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        Err("Indexing not supported for SED-EXACT".to_string())
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

pub fn sed_exact(t1: &SEDIndex, t2: &SEDIndex, threshold: usize) -> usize {
    let first_dist = exact_string_edit_distance(&t1.first_traversal, &t2.first_traversal);
    if first_dist > threshold {
        return first_dist;
    }
    let second_dist = exact_string_edit_distance(&t1.second_traversal, &t2.second_traversal);
    std::cmp::max(first_dist, second_dist)
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
) -> <SedExactAlgorithm as LowerBoundMethod>::PreprocessedDataType {
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

pub struct SedExactFactory;

impl AlgorithmFactory for SedExactFactory {
    type AlgorithmType = SedExactAlgorithm;

    fn create_algorithm() -> Self::AlgorithmType {
        SedExactAlgorithm::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_string_edit_distance() {
        let kitten = "kitten".chars().map(|c| c as i32).collect::<Vec<_>>();
        let sitting = "sitting".chars().map(|c| c as i32).collect::<Vec<_>>();
        assert_eq!(exact_string_edit_distance(&kitten, &sitting), 3);
    }

    #[test]
    fn test_sed_exact_on_indices() {
        let t1 = SEDIndex {
            first_traversal: vec![1, 2, 3],
            second_traversal: vec![2, 3, 1],
            tree_size: 3,
        };
        let t2 = SEDIndex {
            first_traversal: vec![1, 3, 4],
            second_traversal: vec![3, 4, 1],
            tree_size: 3,
        };

        assert_eq!(sed_exact(&t1, &t2, 2), 2);
    }
}
