use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod, TraversalKind, TraversalSelection};
use tree_parsing::{LabelId, ParsedTree};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TraversalCharacter {
    pub char: LabelId,
    pub sum: i32,
    pub diff: i32,
}

#[derive(Debug, Clone)]
pub struct StringStructEDIndex {
    pub first_traversal: Vec<TraversalCharacter>,
    pub second_traversal: Vec<TraversalCharacter>,
    pub tree_size: usize,
}

impl StringStructEDIndex {
    pub fn get_size(&self) -> usize {
        self.tree_size
    }
}

#[derive(Default)]
struct TraversalBuffers {
    preorder: Vec<TraversalCharacter>,
    postorder: Vec<TraversalCharacter>,
    reversed_preorder: Vec<TraversalCharacter>,
    reversed_postorder: Vec<TraversalCharacter>,
}

#[derive(Default)]
pub struct StringStructAlgorithm {
    traversal_selection: TraversalSelection,
}

impl StringStructAlgorithm {
    pub fn new(first: TraversalKind, second: TraversalKind) -> Self {
        Self {
            traversal_selection: TraversalSelection { first, second },
        }
    }
}

impl LowerBoundMethod for StringStructAlgorithm {
    const NAME: &'static str = "SED-STRUCT";
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = StringStructEDIndex;
    type IndexType = ();
    type IndexParams = ();

    fn preprocess(&self, data: &[ParsedTree]) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data
            .iter()
            .map(|t| preprocess_tree(t, self.traversal_selection))
            .collect::<Vec<_>>())
    }

    fn lower_bound(
        &self,
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        sed_struct_k(query, data, threshold)
    }

    fn build_index(
        &self,
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        Err("Indexing not supported for SED-STRUCT".to_string())
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

fn preprocess_tree(tree: &ParsedTree, selection: TraversalSelection) -> StringStructEDIndex {
    let Some(root) = tree.iter().next() else {
        panic!("Unable to get root but tree is not empty!");
    };
    let root_id = tree.get_node_id(root).unwrap();

    let tree_size = tree.count();
    let mut buffers = TraversalBuffers {
        preorder: Vec::new(),
        postorder: Vec::new(),
        reversed_preorder: Vec::new(),
        reversed_postorder: Vec::new(),
    };

    let mut reserve_memory = |kind: TraversalKind| match kind {
        TraversalKind::Preorder => buffers.preorder.reserve(tree.count()),
        TraversalKind::Postorder => buffers.postorder.reserve(tree.count()),
        TraversalKind::ReversedPreorder => buffers.reversed_preorder.reserve(tree.count()),
        TraversalKind::ReversedPostorder => buffers.reversed_postorder.reserve(tree.count()),
    };

    reserve_memory(selection.first);
    reserve_memory(selection.second);

    let mut postorder_id = 0usize;
    let mut depth = 0usize;
    traverse_with_info(
        root_id,
        tree,
        tree_size,
        &mut buffers,
        &mut postorder_id,
        &mut depth,
    );

    buffers.reversed_preorder.reverse();
    buffers.reversed_postorder.reverse();

    StringStructEDIndex {
        first_traversal: pick_traversal(selection.first, &buffers).clone(),
        second_traversal: pick_traversal(selection.second, &buffers).clone(),
        tree_size,
    }
}

fn pick_traversal(kind: TraversalKind, buffers: &TraversalBuffers) -> &Vec<TraversalCharacter> {
    match kind {
        TraversalKind::Preorder => &buffers.preorder,
        TraversalKind::Postorder => &buffers.postorder,
        TraversalKind::ReversedPreorder => &buffers.reversed_preorder,
        TraversalKind::ReversedPostorder => &buffers.reversed_postorder,
    }
}

fn traverse_with_info(
    nid: NodeId,
    tree: &ParsedTree,
    tree_size: usize,
    buffers: &mut TraversalBuffers,
    postorder_id: &mut usize,
    depth: &mut usize,
) -> usize {
    let mut subtree_size = 1;
    *depth += 1;

    let label = tree.get(nid).unwrap().get();

    let pre_idx = buffers.preorder.len();
    buffers.preorder.push(TraversalCharacter {
        char: *label,
        sum: 0,
        diff: 0,
    });
    buffers.reversed_postorder.push(TraversalCharacter {
        char: *label,
        sum: 0,
        diff: 0,
    });

    for cnid in nid.children(tree) {
        subtree_size += traverse_with_info(cnid, tree, tree_size, buffers, postorder_id, depth);
    }

    *depth -= 1;
    *postorder_id += 1;

    let preceding = *postorder_id - subtree_size;
    let following = tree_size - (*postorder_id + *depth);
    let descendant = subtree_size as i32 - 1;
    let ancestor = *depth as i32;

    let pre = buffers.preorder.get_mut(pre_idx).unwrap();
    pre.sum = following as i32 + descendant;
    pre.diff = descendant - following as i32;

    let rev_post = buffers.reversed_postorder.get_mut(pre_idx).unwrap();
    rev_post.sum = preceding as i32 + ancestor;
    rev_post.diff = ancestor - preceding as i32;

    let post_data = TraversalCharacter {
        char: *label,
        sum: following as i32 + ancestor,
        diff: following as i32 - ancestor,
    };

    buffers.postorder.push(post_data.clone());
    buffers.reversed_preorder.push(TraversalCharacter {
        char: *label,
        sum: preceding as i32 + descendant,
        diff: preceding as i32 - descendant,
    });

    subtree_size
}

fn sed_struct_k(t1: &StringStructEDIndex, t2: &StringStructEDIndex, k: usize) -> usize {
    let (mut t1, mut t2) = (t1, t2);
    if t1.get_size().abs_diff(t2.get_size()) > k {
        return k + 1;
    }

    if t1.first_traversal.len() > t2.first_traversal.len() {
        (t1, t2) = (t2, t1);
    }

    let first_dist =
        bounded_string_edit_distance_with_structure(&t1.first_traversal, &t2.first_traversal, k);
    if first_dist > k {
        return first_dist;
    }

    let second_dist =
        bounded_string_edit_distance_with_structure(&t1.second_traversal, &t2.second_traversal, k);

    std::cmp::max(first_dist, second_dist)
}

/// Performs bounded string edit distance with known maximal threshold
/// based on the algorithm by Hal Berghel and David Roach
/// Returns distance at max of K. Algorithm by Hal Berghel and David Roach
/// Assumes size of s2 is bigger or equal than s1
pub fn bounded_string_edit_distance_with_structure(
    s1: &[TraversalCharacter],
    s2: &[TraversalCharacter],
    k: usize,
) -> usize {
    use std::cmp::{max, min};
    // assumes size of s2 is bigger or equal than s1
    let s1len = s1.len() as i32;
    let s2len = s2.len() as i32;
    let size_diff = s2len - s1len;
    // Per Berghel & Roach, the threshold is the min of s2 length and k
    let threshold = min(s2len, k as i32);

    // zero_k represents the initial diagonal (0th/main diagonal of the SED matrix) in the edit distance matrix
    // The shift by 1 and addition of 2 ensures sufficient buffer space
    // as described in the Berghel & Roach paper
    let zero_k: i32 = threshold + 1;

    // Calculate array length needed to store diagonal values
    let array_size = (2 * threshold + 3) as usize;

    // Instead of storing the full DP matrix, Ukkonen's algorithm only stores
    // the current and next row (optimization described in the paper)
    let mut current_row = vec![(-1i32, true); array_size as usize];
    let mut next_row = vec![(-1i32, true); array_size as usize];
    // condition_diagonal is the diaogonal on which the resulting SED lies.
    // we will be checking this diagonal to determine if we can stop early
    let target_diagonal = size_diff + zero_k;
    let target_diagonal_idx = target_diagonal as usize;
    let end_max = target_diagonal << 1;

    // i => p in the C[p, k]
    // k => is the target
    for i in 1..=threshold + 1 {
        std::mem::swap(&mut next_row, &mut current_row);

        // Calculate original band boundaries from Berghel-Roach algorithm
        let original_start: i32 = if i <= zero_k {
            -i + 1
        } else {
            i - (zero_k << 1) + 1
        };

        let original_end: i32;
        if i <= target_diagonal {
            original_end = i;
            unsafe {
                *next_row.get_unchecked_mut((zero_k + i) as usize) = (-1, true);
            }
        } else {
            original_end = end_max - i;
        }

        // Precompute valid diagonal range based on budget
        // Use k (not threshold) for budget calculation since k is the actual distance limit
        let budget = k as i32 - (i - 1);

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
        let mut current_cell: i32;
        let mut next_cell: i32;
        let mut previous_cell: i32;
        let mut next_allowed_substitution: bool;

        // Load initial values from previous row based on adjusted start position
        if i <= zero_k && start == original_start {
            // Original initialization for the standard case
            current_cell = -1;
            next_cell = i - 2i32;
            next_allowed_substitution = true;
        } else {
            // When start is adjusted, load values from the appropriate positions
            unsafe {
                let start_idx = (zero_k + start) as usize;
                current_cell = if start > original_start && start_idx > 0 {
                    current_row.get_unchecked(start_idx - 1).0
                } else {
                    -1
                };
                (next_cell, next_allowed_substitution) = *current_row.get_unchecked(start_idx);
            }
        }

        let mut diagonal_index: usize = (start + zero_k).try_into().unwrap();

        let mut max_row_number;
        let allowed_edits = i - 1;

        // Process each diagonal in the band for this iteration
        let mut can_substitute: bool;
        for diag_offset in start..end {
            // Per Ukkonen's algorithm, we're tracking three values to compute each cell:
            // previous_cell, current_cell, and next_cell from the previous row

            // f(d-1, p-1) - insertion - row remains
            previous_cell = current_cell;
            // f(d, p-1) - substitution of character
            current_cell = next_cell;
            can_substitute = next_allowed_substitution;
            unsafe {
                // f(d+1, p-1) - deletion - max row index adds by +1
                (next_cell, next_allowed_substitution) =
                    *current_row.get_unchecked(diagonal_index + 1);
            }

            // Calculate the max of three possible operations (delete, insert, replace)
            // This is the standard dynamic programming recurrence relation for edit distance
            // however replacement can not occur in all cases, only if the mapping is possible
            // current_cell is basically the row in the matrix

            unsafe {
                // do a current_cell + 1
                // If substitution is not allowed, treat as insertion/deletion (not diagonal move) current_cell + 0
                max_row_number = max(
                    current_cell + (if can_substitute { 1 } else { 0 }),
                    max(previous_cell, next_cell + 1),
                );

                if !can_substitute {
                    // pokud nemuzu delat substituci a previous a next nedaji vetsi cislo, tak jen vezmu cislo
                    // current_cell, rovnou zapisu a nemusim se ani pokouset delat extension - zda se mi to zvetsi

                    if max_row_number == current_cell {
                        *next_row.get_unchecked_mut(diagonal_index) = (max_row_number, false);
                        diagonal_index += 1;
                        continue;
                    }
                }
            }
            unsafe {
                let k = k as i32;
                // The core extension to the original algorithm: match characters while possible
                // and consider both character equality AND structural constraints
                // This is the diagonal extension from Ukkonen's algorithm

                // Branchless optimization: Instead of breaking on structural constraint violation,
                // we compute how many characters we can advance before hitting the constraint.
                // This eliminates the inner branch and reduces pipeline stalls.

                // First, find the maximum possible advance based on character equality

                let mut struct_ok = false;

                // Optimized: fetch once, reuse
                while max_row_number < s1len && (max_row_number + diag_offset) < s2len {
                    let c1 = s1.get_unchecked(max_row_number as usize);
                    let c2 = s2.get_unchecked((max_row_number + diag_offset) as usize);

                    let char_eq = c1.char == c2.char;
                    struct_ok = (allowed_edits + (c1.sum - c2.sum).abs() <= k)
                        && (allowed_edits + (c1.diff - c2.diff).abs() <= k);

                    if !char_eq || !struct_ok {
                        break;
                    }
                    max_row_number += 1;
                }

                // Update substitution flag without branching: can substitute if we matched all characters
                // that were equal (no structural constraint violation occurred)
                *next_row.get_unchecked_mut(diagonal_index) = (max_row_number, struct_ok);
            }

            diagonal_index += 1;
        }

        // Check termination condition: either we've computed enough rows
        // to determine the distance is > threshold, or we've reached the
        // threshold itself - this follows the "cutoff" principle in the paper
        unsafe {
            if next_row.get_unchecked(target_diagonal_idx).0 >= s1len {
                return (i - 1) as usize;
            }
        }
    }

    usize::MAX
}

pub struct StringStructFactory;

impl AlgorithmFactory for StringStructFactory {
    type AlgorithmType = StringStructAlgorithm;
    fn create_algorithm() -> Self::AlgorithmType {
        StringStructAlgorithm::default()
    }
}

#[cfg(test)]
mod tests {
    use tree_parsing::{parse_single, LabelDict};

    use super::*;

    #[test]
    fn test_bounded_sed_structure() {
        // arvey
        let v1 = vec![
            TraversalCharacter {
                char: 1,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 2,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 3,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 4,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 5,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 6,
                sum: 0,
                diff: 0,
            },
        ];
        // avery
        let v2 = vec![
            TraversalCharacter {
                char: 2,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 3,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 4,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 5,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 5,
                sum: 0,
                diff: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 3);
        assert_eq!(result, 2);
    }

    #[test]
    fn test_bounded_sed_structure_2() {
        // sitting
        let v1 = vec![
            TraversalCharacter {
                char: 1,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 3,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 4,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 4,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 3,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 6,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 7,
                sum: 0,
                diff: 0,
            },
        ];
        // kitten
        let v2 = vec![
            TraversalCharacter {
                char: 2,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 3,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 4,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 4,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 5,
                sum: 0,
                diff: 0,
            },
            TraversalCharacter {
                char: 6,
                sum: 0,
                diff: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 3);
        assert_eq!(result, 3);
    }

    #[test]
    fn test_sed_preorder_structure() {
        let t1str = "{a{a{b{a{a}}}}}".to_owned();
        let t2str = "{a{b{b{b}}{a{a}}}}".to_owned();
        let mut ld = LabelDict::new();
        let qt = parse_single(t1str, &mut ld);
        let tt = parse_single(t2str, &mut ld);
        let result = StringStructAlgorithm {
            traversal_selection: TraversalSelection::default(),
        }
        .preprocess(&[qt.clone(), tt.clone()])
        .unwrap();

        let [qs, ts, ..] = result.as_slice() else {
            panic!("Expected at least 2 elements");
        };

        assert_eq!(
            qs.first_traversal,
            vec![
                TraversalCharacter {
                    char: 1,
                    sum: 4,
                    diff: 4,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 3,
                    diff: 3,
                },
                TraversalCharacter {
                    char: 2,
                    sum: 2,
                    diff: 2,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 1,
                    diff: 1,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 0,
                    diff: 0,
                },
            ]
        );

        assert_eq!(
            qs.second_traversal,
            vec![
                TraversalCharacter {
                    char: 1,
                    sum: 4,
                    diff: -4,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 3,
                    diff: -3,
                },
                TraversalCharacter {
                    char: 2,
                    sum: 2,
                    diff: -2,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 1,
                    diff: -1,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 0,
                    diff: 0,
                },
            ]
        );

        assert_eq!(
            ts.first_traversal,
            vec![
                TraversalCharacter {
                    char: 1,
                    sum: 5,
                    diff: 5,
                },
                TraversalCharacter {
                    char: 2,
                    sum: 4,
                    diff: 4,
                },
                TraversalCharacter {
                    char: 2,
                    sum: 3,
                    diff: -1,
                },
                TraversalCharacter {
                    char: 2,
                    sum: 2,
                    diff: -2,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 1,
                    diff: 1,
                },
                TraversalCharacter {
                    char: 1,
                    sum: 0,
                    diff: 0,
                },
            ]
        );
    }
}
