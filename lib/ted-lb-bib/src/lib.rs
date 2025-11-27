use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use tree_parsing::{LabelId, ParsedTree};

use itertools::Itertools;
use rustc_hash::FxHashMap;
use std::cmp::min;

pub type BinaryBranchVector = FxHashMap<i32, i32>;
pub struct BinaryBranchTree {
    pub size: usize,
    pub branch_vector: BinaryBranchVector,
}

// Binary branch tuple (root label, left label, right label)
type BBTuple = (LabelId, Option<LabelId>, Option<LabelId>);

#[derive(Default, Debug)]
pub struct BinaryBranchLowerBoundMethod {
    bb_id: i32,
    binary_branch_id_map: FxHashMap<BBTuple, i32>,
}

impl LowerBoundMethod for BinaryBranchLowerBoundMethod {
    const NAME: &'static str = "BinaryBranch";
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = BinaryBranchTree;
    type PreprocessParams = ();
    type IndexType = ();
    type IndexParams = ();

    fn preprocess(
        &mut self,
        data: &[ParsedTree],
        _params: Self::PreprocessParams,
    ) -> Result<Vec<Self::PreprocessedDataType>, String> {
        // Placeholder implementation
        Ok(data
            .iter()
            .map(|tree| {
                let Some(root) = tree.iter().next() else {
                    panic!("tree is empty");
                };
                let root_id = tree.get_node_id(root).unwrap();
                let mut branch_vector = BinaryBranchVector::default();
                self.create_vector(&root_id, tree, None, &mut branch_vector);
                BinaryBranchTree {
                    size: tree.count(),
                    branch_vector,
                }
            })
            .collect_vec())
    }

    fn lower_bound(
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        let (t1s, t2s) = (data.size, query.size);
        if t1s.abs_diff(t2s) > threshold {
            return threshold + 1;
        }
        let mut intersection_size = 0usize;

        for (label, postings) in data.branch_vector.iter() {
            let Some(t2postings) = query.branch_vector.get(label) else {
                continue;
            };
            intersection_size += min(*t2postings, *postings) as usize;
        }

        ((t1s + t2s) - (2 * intersection_size)) / 5
    }

    fn build_index(
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        // Placeholder implementation
        unimplemented!("Indexing not implemented yet")
    }

    fn query_index(
        _query: &Self::PreprocessedDataType,
        _index: &Self::IndexType,
        _threshold: usize,
    ) -> Vec<usize> {
        unimplemented!("Index querying not implemented yet")
    }
}

impl BinaryBranchLowerBoundMethod {
    fn create_vector(
        &mut self,
        root_id: &NodeId,
        tree: &ParsedTree,
        right_sibling_label: Option<LabelId>,
        branch_vector: &mut BinaryBranchVector,
    ) {
        let children = root_id.children(tree).collect_vec();
        let mut left_label = None;
        if let Some(left_child) = children.first() {
            left_label = Some(*tree.get(*left_child).unwrap().get())
        }

        let bb_tuple: BBTuple = (
            *tree.get(*root_id).unwrap().get(),
            left_label,
            right_sibling_label,
        );

        let bb_id = self
            .binary_branch_id_map
            .entry(bb_tuple)
            .or_insert_with(|| {
                self.bb_id += 1;
                self.bb_id
            });

        branch_vector
            .entry(*bb_id)
            .and_modify(|count| *count += 1)
            .or_insert(1);

        for (i, cnode) in children.iter().enumerate() {
            let right_sibling_l = if i < children.len() - 1 {
                Some(*tree.get(children[i + 1]).unwrap().get())
            } else {
                None
            };
            self.create_vector(cnode, tree, right_sibling_l, branch_vector);
        }
    }
}

pub struct BinaryBranchLowerBoundFactory;

impl AlgorithmFactory for BinaryBranchLowerBoundFactory {
    fn create_algorithm(&self, algo_type: ted_base::AlgorithmType) -> impl LowerBoundMethod {
        match algo_type {
            ted_base::AlgorithmType::BinaryBranch => BinaryBranchLowerBoundMethod::default(),
            _ => panic!("Unsupported algorithm type for StructuralLowerBoundFactory"),
        }
    }
}
