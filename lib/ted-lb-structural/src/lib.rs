use indextree::NodeId;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use tree_parsing::LabelId;

pub struct StructuralLowerBoundMethod;

// preceding
const REGION_LEFT_IDX: usize = 0;
/// ancestors
const REGION_ANC_IDX: usize = 1;
// following
const REGION_RIGHT_IDX: usize = 2;
/// descendants
const REGION_DESC_IDX: usize = 3;

type StructuralRegionType = i32;

pub struct StructuralVector {
    /// Vector of number of nodes to the left (preceding), ancestors, nodes to right (following) and descendants
    pub regions: [StructuralRegionType; 4],
    pub postorder_id: usize,
}

type LabelMap = FxHashMap<LabelId, Vec<StructuralVector>>;

pub struct StructuralLabelMap {
    tree_size: usize,
    label_map: LabelMap,
}

impl LowerBoundMethod for StructuralLowerBoundMethod {
    const NAME: &'static str = "Structural";
    // This method does support index-based computations, but it's not implemented yet.
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = StructuralLabelMap;
    type IndexType = ();
    type IndexParams = ();
    type PreprocessParams = ();

    fn lower_bound(
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        unimplemented!("Lower bound computation not implemented for StructuralLowerBoundMethod");
    }

    fn preprocess(
        &mut self,
        data: &[tree_parsing::ParsedTree],
        params: Self::PreprocessParams,
    ) -> Result<Vec<Self::PreprocessedDataType>, String> {
        // TODO: Implement preprocessing to create StructuralLabelMap for each tree
        // contains structural vectors for the current tree
        // is it a hash map of Label -> Vec<StructVec>
        Ok(data
            .iter()
            .map(|tree| {
                let mut actual_depth: StructuralRegionType = 0;
                let mut actual_post_order_number: StructuralRegionType = 0;
                let mut postorder_id = 0usize;

                let Some(root) = tree.iter().next() else {
                    panic!("tree is empty");
                };
                let root_id = tree.get_node_id(root).expect("Root node msut exist");
                let mut record_labels = LabelMap::default();

                create_record(
                    &root_id,
                    tree,
                    &mut postorder_id,
                    &mut actual_depth,
                    &mut actual_post_order_number,
                    &mut record_labels,
                );

                Self::PreprocessedDataType {
                    tree_size: tree.count(),
                    label_map: record_labels,
                }
            })
            .collect_vec())
    }

    fn query_index(
        query: &Self::PreprocessedDataType,
        index: &Self::IndexType,
        threshold: usize,
    ) -> Vec<usize> {
        unimplemented!("Index querying not implemented for StructuralLowerBoundMethod");
    }

    fn build_index(
        data: &[Self::PreprocessedDataType],
        params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        unimplemented!("Index building not implemented for StructuralLowerBoundMethod");
    }
}

fn create_record(
    root_id: &NodeId,
    tree: &tree_parsing::ParsedTree,
    postorder_id: &mut usize,
    actual_depth: &mut StructuralRegionType,
    actual_post_order_number: &mut StructuralRegionType,
    record_labels: &mut LabelMap,
) -> StructuralRegionType {
    // number of children = subtree_size - 1
    // subtree_size = 1 -> actual node + sum of children
    let mut subtree_size = 1;

    *actual_depth += 1;

    for cid in root_id.children(tree) {
        subtree_size += create_record(
            &cid,
            tree,
            postorder_id,
            actual_depth,
            actual_post_order_number,
            record_labels,
        );
    }

    *postorder_id += 1;
    *actual_depth -= 1;
    *actual_post_order_number += 1;

    let root_label = tree.get(*root_id).unwrap().get();
    let node_struct_vec = StructuralVector {
        postorder_id: *postorder_id,
        regions: [
            (*actual_post_order_number - subtree_size),
            *actual_depth,
            (tree.count() as StructuralRegionType - (*actual_post_order_number + *actual_depth)),
            (subtree_size - 1),
        ],
    };

    if let Some(label_nodes) = record_labels.get_mut(root_label) {
        label_nodes.push(node_struct_vec);
    } else {
        record_labels.insert(*root_label, vec![node_struct_vec]);
    }
    subtree_size
}

pub struct StructuralLowerBoundFactory;

impl AlgorithmFactory for StructuralLowerBoundFactory {
    fn create_algorithm(&self, algo_type: ted_base::AlgorithmType) -> impl LowerBoundMethod {
        match algo_type {
            ted_base::AlgorithmType::Structural => StructuralLowerBoundMethod {},
            _ => panic!("Unsupported algorithm type for StructuralLowerBoundFactory"),
        }
    }
}
