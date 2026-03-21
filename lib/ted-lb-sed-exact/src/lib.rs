use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use ted_lb_sed::SEDIndex;
use tree_parsing::ParsedTree;

pub struct SedExactAlgorithm;

impl LowerBoundMethod for SedExactAlgorithm {
    const NAME: &'static str = "SED-EXACT";
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = SEDIndex;
    type IndexType = ();
    type IndexParams = ();

    fn preprocess(&self, data: &[ParsedTree]) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data.iter().map(preprocess_tree).collect::<Vec<_>>())
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
    let post_dist = exact_string_edit_distance(&t1.postorder, &t2.postorder);
    if post_dist > threshold {
        return post_dist;
    }
    let pre_dist = exact_string_edit_distance(&t1.preorder, &t2.preorder);
    std::cmp::max(pre_dist, post_dist)
}

pub fn exact_string_edit_distance(s1: &[i32], s2: &[i32]) -> usize {
    if s1.is_empty() {
        return s2.len();
    }
    if s2.is_empty() {
        return s1.len();
    }

    let (a, b) = if s1.len() <= s2.len() {
        (s1, s2)
    } else {
        (s2, s1)
    };

    let mut prev = (0..=a.len()).collect::<Vec<usize>>();
    let mut curr = vec![0usize; a.len() + 1];

    for (i, bch) in b.iter().enumerate() {
        curr[0] = i + 1;
        for (j, ach) in a.iter().enumerate() {
            let cost = if ach == bch { 0 } else { 1 };
            let deletion = prev[j + 1] + 1;
            let insertion = curr[j] + 1;
            let substitution = prev[j] + cost;
            curr[j + 1] = deletion.min(insertion).min(substitution);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[a.len()]
}

fn preprocess_tree(
    tree: &ParsedTree,
) -> <SedExactAlgorithm as LowerBoundMethod>::PreprocessedDataType {
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

fn traverse(nid: NodeId, tree: &ParsedTree, pre: &mut Vec<i32>, post: &mut Vec<i32>) {
    let label = tree.get(nid).unwrap().get();
    pre.push(*label);
    for cnid in nid.children(tree) {
        traverse(cnid, tree, pre, post);
    }
    post.push(*label);
}

pub struct SedExactFactory;

impl AlgorithmFactory for SedExactFactory {
    type AlgorithmType = SedExactAlgorithm;

    fn create_algorithm() -> Self::AlgorithmType {
        SedExactAlgorithm
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
            preorder: vec![1, 2, 3],
            postorder: vec![2, 3, 1],
            tree_size: 3,
        };
        let t2 = SEDIndex {
            preorder: vec![1, 3, 4],
            postorder: vec![3, 4, 1],
            tree_size: 3,
        };

        assert_eq!(sed_exact(&t1, &t2, 2), 2);
    }
}
