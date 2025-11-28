use core::str;

use rustc_hash::FxHashMap;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use tree_parsing::{LabelId, ParsedTree};

pub struct LabelIntersectionAlgorithm;

pub struct LabelCountMap {
    inv_index: FxHashMap<LabelId, i32>,
    tree_size: usize,
}

impl LowerBoundMethod for LabelIntersectionAlgorithm {
    const NAME: &'static str = "LabelIntersection";
    // TODO: it does support index, but not yet implemented
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = LabelCountMap;
    type IndexType = ();
    type IndexParams = ();
    type PreprocessParams = ();

    fn preprocess(
        &mut self,
        data: &[ParsedTree],
        _params: Self::PreprocessParams,
    ) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data.iter().map(preprocess_tree).collect())
    }

    fn lower_bound(
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        use std::cmp::{max, min};
        let mut intersection_size = 0;
        let bigger_tree = max(query.tree_size, data.tree_size);

        if query.tree_size.abs_diff(data.tree_size) > threshold {
            return threshold + 1;
        }

        for (label, &q_count) in &query.inv_index {
            if let Some(&d_count) = data.inv_index.get(label) {
                intersection_size += min(q_count, d_count);
            }

            if bigger_tree - (intersection_size as usize) < threshold {
                return bigger_tree - intersection_size as usize;
            }
        }

        max(query.tree_size, data.tree_size) - intersection_size as usize
    }

    fn build_index(
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        Ok(())
    }

    fn query_index(
        _query: &Self::PreprocessedDataType,
        _index: &Self::IndexType,
        _threshold: usize,
    ) -> Vec<usize> {
        vec![]
    }
}

fn preprocess_tree(
    tree: &ParsedTree,
) -> <LabelIntersectionAlgorithm as LowerBoundMethod>::PreprocessedDataType {
    let mut label_count: FxHashMap<LabelId, i32> = FxHashMap::default();

    let Some(root) = tree.iter().next() else {
        panic!("Unable to get root but tree is not empty!");
    };
    let root_id = tree.get_node_id(root).expect("Failed to get root node id");

    for nid in root_id.descendants(tree) {
        let label = tree.get(nid).expect("Failed to get node by it's id").get();
        label_count
            .entry(*label)
            .and_modify(|label_count| {
                *label_count += 1;
            })
            .or_insert(1);
    }

    LabelCountMap {
        inv_index: label_count,
        tree_size: tree.count(),
    }
}

pub struct LabelIntersectionFactory;

impl AlgorithmFactory for LabelIntersectionFactory {
    type AlgorithmType = LabelIntersectionAlgorithm;
    fn create_algorithm() -> Self::AlgorithmType {
        LabelIntersectionAlgorithm
    }
}
