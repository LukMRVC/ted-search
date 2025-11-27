use indextree::NodeId;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use tree_parsing::LabelId;

pub struct StructuralLowerBoundMethod;

type StructuralRegionType = i32;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StructuralVector {
    /// Vector of number of nodes to the left (preceding), ancestors, nodes to right (following) and descendants
    pub regions: [StructuralRegionType; 4],
    pub postorder_id: usize,
}

type LabelMap = FxHashMap<LabelId, Vec<StructuralVector>>;

#[derive(Debug, PartialEq, Eq, Clone)]
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
        ted(query, data, threshold)
    }

    fn preprocess(
        &mut self,
        data: &[tree_parsing::ParsedTree],
        _params: Self::PreprocessParams,
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
        _query: &Self::PreprocessedDataType,
        _index: &Self::IndexType,
        _threshold: usize,
    ) -> Vec<usize> {
        unimplemented!("Index querying not implemented for StructuralLowerBoundMethod");
    }

    fn build_index(
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
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

#[inline(always)]
fn svec_l1_strict(n1: &[StructuralRegionType; 4], n2: &[StructuralRegionType; 4]) -> i32 {
    n1.iter()
        .zip_eq(n2.iter())
        .fold(0, |acc, (a, b)| acc + (a - b).abs())
}

/// Given two sets
pub fn ted(s1: &StructuralLabelMap, s2: &StructuralLabelMap, k: usize) -> usize {
    use std::cmp::max;
    // simple size difference
    let bigger = max(s1.tree_size, s2.tree_size);
    if s1.tree_size.abs_diff(s2.tree_size) > k {
        return k + 1;
    }
    let k = k as i32;

    let mut overlap = 0;
    for (lblid, set1) in s1.label_map.iter() {
        if let Some(set2) = s2.label_map.get(lblid) {
            if set1.len() == 1 && set2.len() == 1 {
                let l1_region_distance = svec_l1_strict(
                    &set1
                        .first()
                        .expect("Failed to get first vector element!")
                        .regions,
                    &set2
                        .first()
                        .expect("Failed to get first vector element!")
                        .regions,
                );

                if l1_region_distance <= k {
                    overlap += 1;
                }
                continue;
            }

            let (s1c, s2c) = if set2.len() < set1.len() {
                (set2, set1)
            } else {
                (set1, set2)
            };

            for n1 in s1c.iter() {
                let k_window = n1.postorder_id as i32 - k;
                let k_window = std::cmp::max(k_window, 0) as usize;

                // apply postorder filter
                // let s2clen = s2c.struct_vec.len();
                for n2 in s2c
                    .iter()
                    .skip_while(|n2| k_window < s2c.len() && n2.postorder_id < k_window)
                    .take_while(|n2| n2.postorder_id <= k as usize + n1.postorder_id)
                {
                    let l1_region_distance = svec_l1_strict(&n1.regions, &n2.regions);

                    if l1_region_distance <= k {
                        overlap += 1;
                        break;
                    }
                }
            }
        }
    }

    bigger - overlap
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

#[cfg(test)]
mod tests {
    use super::*;
    use tree_parsing::{parse_single, LabelDict};

    #[test]
    fn test_label_set_converting() {
        let t1input = "{a{b{b{a}}{a}}}".to_owned();
        let mut label_dict = LabelDict::default();
        let t1 = parse_single(t1input, &mut label_dict);

        let mut lb_method = StructuralLowerBoundMethod {};

        let preprocessed = lb_method
            .preprocess(&[t1], ())
            .expect("unable to preprocess tree");

        let single_tree = preprocessed.first().unwrap();

        let structural_tree = LabelMap::from_iter([
            (
                1,
                vec![
                    StructuralVector {
                        regions: [0, 3, 1, 0],
                        postorder_id: 1,
                    },
                    StructuralVector {
                        regions: [2, 2, 0, 0],
                        postorder_id: 3,
                    },
                    StructuralVector {
                        regions: [0, 0, 0, 4],
                        postorder_id: 5,
                    },
                ],
            ),
            (
                2,
                vec![
                    StructuralVector {
                        regions: [0, 2, 1, 1],
                        postorder_id: 2,
                    },
                    StructuralVector {
                        regions: [0, 1, 0, 3],
                        postorder_id: 4,
                    },
                ],
            ),
        ]);

        assert_eq!(single_tree.label_map, structural_tree);
    }

    #[test]
    fn test_label_set_converting_second() {
        let t1input = "{a{b{a}{b{a}}}}".to_owned();

        let mut label_dict = LabelDict::default();
        let t1 = parse_single(t1input, &mut label_dict);

        let mut lb_method = StructuralLowerBoundMethod {};

        let preprocessed = lb_method
            .preprocess(&[t1], ())
            .expect("unable to preprocess tree");

        let single_tree = preprocessed.first().unwrap();

        let structural_tree = LabelMap::from_iter([
            (
                1,
                vec![
                    StructuralVector {
                        regions: [0, 2, 2, 0],
                        postorder_id: 1,
                    },
                    StructuralVector {
                        regions: [1, 3, 0, 0],
                        postorder_id: 2,
                    },
                    StructuralVector {
                        regions: [0, 0, 0, 4],
                        postorder_id: 5,
                    },
                ],
            ),
            (
                2,
                vec![
                    StructuralVector {
                        regions: [1, 2, 0, 1],
                        postorder_id: 3,
                    },
                    StructuralVector {
                        regions: [0, 1, 0, 3],
                        postorder_id: 4,
                    },
                ],
            ),
        ]);

        assert_eq!(single_tree.label_map, structural_tree);
        // assert_eq!(set_tuple.label_map.get(&2).unwrap(), &lse_for_b);
    }

    #[test]
    fn test_structural_distance() {
        let t1input = "{a{b}{a{b}{c}{a}}{b}}".to_owned();
        let t2input = "{a{c}{b{a{a}{b}{c}}}}".to_owned();
        let mut label_dict = LabelDict::default();
        let t1 = parse_single(t1input, &mut label_dict);
        let t2 = parse_single(t2input, &mut label_dict);
        let v = vec![t1, t2];
        let mut lb_method = StructuralLowerBoundMethod {};
        let preprocessed = lb_method
            .preprocess(&v, ())
            .expect("unable to preprocess tree");

        let lb = StructuralLowerBoundMethod::lower_bound(&preprocessed[0], &preprocessed[1], 4);

        assert_eq!(lb, 2);
    }
}
