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

pub struct StructuralVector {
    /// Vector of number of nodes to the left (preceding), ancestors, nodes to right (following) and descendants
    pub regions: [usize; 4],
    pub postorder_id: usize,
}

pub struct StructuralLabelMap {
    tree_size: usize,
    label_map: FxHashMap<LabelId, Vec<StructuralVector>>,
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
        let mut record_labels = StructHashMap::default();

        let Some(root) = tree.iter().next() else {
            panic!("tree is empty");
        };
        let root_id = tree.get_node_id(root).unwrap();
        // for recursive postorder traversal
        let mut postorder_id = 0;

        self.tree_size_by_split_id[0] = tree.count() as RegionNumType;

        // array of records stored in sets_collection
        self.create_record(&root_id, tree, &mut postorder_id, &mut record_labels);
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
    &mut self,
    root_id: &NodeId,
    tree: &ParsedTree,
    postorder_id: &mut usize,
    record_labels: &mut StructHashMap,
) -> RegionNumType {
    // number of children = subtree_size - 1
    // subtree_size = 1 -> actual node + sum of children
    let mut subtree_size = 1;

    self.actual_depth[0] += 1;

    for cid in root_id.children(tree) {
        subtree_size += self.create_record(&cid, tree, postorder_id, record_labels);
    }

    *postorder_id += 1;
    self.actual_depth[0] -= 1;
    self.actual_post_order_number[0] += 1;

    let root_label = tree.get(*root_id).unwrap().get();
    let node_struct_vec = StructuralVec {
        postorder_id: *postorder_id,
        label_id: *root_label,
        mapping_regions: [
            (self.actual_post_order_number[0] - subtree_size),
            self.actual_depth[0],
            (self.tree_size_by_split_id[0]
                - (self.actual_post_order_number[0] + self.actual_depth[0])),
            (subtree_size - 1),
        ],
    };

    if let Some(se) = record_labels.get_mut(root_label) {
        se.base.weight += 1;
        se.struct_vec.push(node_struct_vec);
    } else {
        let mut se = LabelSetElement {
            base: LabelSetElementBase {
                id: *tree.get(*root_id).unwrap().get(),
                weight: 1,
                ..LabelSetElementBase::default()
            },
            ..LabelSetElement::default()
        };
        se.struct_vec.push(node_struct_vec);
        record_labels.insert(*root_label, se);
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
