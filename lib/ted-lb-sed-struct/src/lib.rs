use indextree::NodeId;
use ted_base::{AlgorithmFactory, LowerBoundMethod};
use tree_parsing::{LabelId, ParsedTree};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TraversalCharacter {
    pub char: LabelId,
    pub preorder_following_postorder_preceding: i32,
    pub preorder_descendant_postorder_ancestor: i32,
}

#[macro_export]
macro_rules! traversal_enum {
    (
        $(
            $variant_name:ident: [$($field_name:ident),* $(,)?]
        ),* $(,)?
    ) => {
        #[derive(Debug)]
        pub enum StringStructEDIndex {
            $(
                $variant_name {
                    $(
                        $field_name: Vec<TraversalCharacter>,
                    )*
                    tree_size: usize,
                }
            ),*
        }
    };
}

traversal_enum! {
    PreRevPre: [preorder, reversed_preorder],
    PostRevPost: [postorder, reversed_postorder],
    PostRevPre: [postorder, reversed_preorder],
    PreRevPost: [preorder, reversed_postorder],
    AllTraversals: [preorder, postorder, reversed_preorder, reversed_postorder],
}

impl StringStructEDIndex {
    pub fn new(of_type: &TraversalType, tree_size: usize) -> Self {
        use TraversalType::*;
        match of_type {
            PreRevPre => StringStructEDIndex::PreRevPre {
                preorder: Vec::with_capacity(tree_size),
                reversed_preorder: Vec::with_capacity(tree_size),
                tree_size: tree_size,
            },
            PostRevPost => StringStructEDIndex::PostRevPost {
                postorder: Vec::with_capacity(tree_size),
                reversed_postorder: Vec::with_capacity(tree_size),
                tree_size: tree_size,
            },
            PostRevPre => StringStructEDIndex::PostRevPre {
                postorder: Vec::with_capacity(tree_size),
                reversed_preorder: Vec::with_capacity(tree_size),
                tree_size: tree_size,
            },
            PreRevPost => StringStructEDIndex::PreRevPost {
                preorder: Vec::with_capacity(tree_size),
                reversed_postorder: Vec::with_capacity(tree_size),
                tree_size: tree_size,
            },
            AllTraversals => StringStructEDIndex::AllTraversals {
                preorder: Vec::with_capacity(tree_size),
                postorder: Vec::with_capacity(tree_size),
                reversed_preorder: Vec::with_capacity(tree_size),
                reversed_postorder: Vec::with_capacity(tree_size),
                tree_size: tree_size,
            },
        }
    }

    pub fn get_size(&self) -> usize {
        match self {
            StringStructEDIndex::PreRevPre { tree_size, .. } => *tree_size,
            StringStructEDIndex::PostRevPost { tree_size, .. } => *tree_size,
            StringStructEDIndex::PostRevPre { tree_size, .. } => *tree_size,
            StringStructEDIndex::PreRevPost { tree_size, .. } => *tree_size,
            StringStructEDIndex::AllTraversals { tree_size, .. } => *tree_size,
        }
    }

    pub(crate) fn reverse_if_needed(&mut self) {
        match self {
            StringStructEDIndex::PreRevPre {
                reversed_preorder, ..
            } => reversed_preorder.reverse(),
            StringStructEDIndex::PostRevPost {
                reversed_postorder, ..
            } => reversed_postorder.reverse(),
            StringStructEDIndex::PostRevPre {
                reversed_preorder, ..
            } => reversed_preorder.reverse(),
            StringStructEDIndex::PreRevPost {
                reversed_postorder, ..
            } => reversed_postorder.reverse(),
            StringStructEDIndex::AllTraversals {
                reversed_preorder,
                reversed_postorder,
                ..
            } => {
                reversed_preorder.reverse();
                reversed_postorder.reverse();
            }
        }
    }

    pub(crate) fn get_pre_len(&self) -> usize {
        match self {
            StringStructEDIndex::PreRevPre { preorder, .. } => preorder.len(),
            StringStructEDIndex::PostRevPost {
                reversed_postorder, ..
            } => reversed_postorder.len(),
            StringStructEDIndex::PostRevPre { .. } => 1, // so that if I subtract -1 I don't underflow or panic
            StringStructEDIndex::PreRevPost { preorder, .. } => preorder.len(),
            StringStructEDIndex::AllTraversals { preorder, .. } => preorder.len(),
        }
    }

    pub(crate) fn push_pre_data(&mut self, data: TraversalCharacter) {
        match self {
            StringStructEDIndex::PreRevPre { preorder, .. } => {
                preorder.push(data);
            }
            StringStructEDIndex::PostRevPost {
                reversed_postorder, ..
            } => reversed_postorder.push(data),
            StringStructEDIndex::PostRevPre { .. } => {}
            StringStructEDIndex::PreRevPost {
                preorder,
                reversed_postorder,
                ..
            } => {
                preorder.push(data.clone());
                reversed_postorder.push(data);
            }
            StringStructEDIndex::AllTraversals {
                preorder,
                reversed_postorder,
                ..
            } => {
                preorder.push(data.clone());
                reversed_postorder.push(data);
            }
        }
    }

    pub(crate) fn push_post_data(&mut self, data: TraversalCharacter) {
        match self {
            StringStructEDIndex::PreRevPre {
                reversed_preorder, ..
            } => {
                reversed_preorder.push(data);
            }
            StringStructEDIndex::PostRevPost { postorder, .. } => postorder.push(data),
            StringStructEDIndex::PostRevPre {
                postorder,
                reversed_preorder,
                ..
            } => {
                postorder.push(data.clone());
                reversed_preorder.push(data);
            }
            StringStructEDIndex::PreRevPost { .. } => {}
            StringStructEDIndex::AllTraversals {
                postorder,
                reversed_preorder,
                ..
            } => {
                postorder.push(data.clone());
                reversed_preorder.push(data);
            }
        }
    }

    pub(crate) fn set_pre_data(
        &mut self,
        idx: usize,
        following: i32,
        descendant: i32,
        preceding: i32,
        ancestor: i32,
    ) {
        match self {
            StringStructEDIndex::PreRevPre { preorder, .. } => {
                let element = preorder.get_mut(idx).unwrap();
                element.preorder_following_postorder_preceding = following;
                element.preorder_descendant_postorder_ancestor = descendant;
            }
            StringStructEDIndex::PostRevPost {
                reversed_postorder, ..
            } => {
                let element = reversed_postorder.get_mut(idx).unwrap();
                element.preorder_following_postorder_preceding = preceding;
                element.preorder_descendant_postorder_ancestor = ancestor;
            }
            StringStructEDIndex::PostRevPre { .. } => {}
            StringStructEDIndex::PreRevPost {
                preorder,
                reversed_postorder,
                ..
            } => {
                let element = preorder.get_mut(idx).unwrap();
                element.preorder_following_postorder_preceding = following;
                element.preorder_descendant_postorder_ancestor = descendant;

                let element = reversed_postorder.get_mut(idx).unwrap();
                element.preorder_following_postorder_preceding = preceding;
                element.preorder_descendant_postorder_ancestor = ancestor;
            }
            StringStructEDIndex::AllTraversals {
                preorder,
                reversed_postorder,
                ..
            } => {
                let element = preorder.get_mut(idx).unwrap();
                element.preorder_following_postorder_preceding = following;
                element.preorder_descendant_postorder_ancestor = descendant;

                let element = reversed_postorder.get_mut(idx).unwrap();
                element.preorder_following_postorder_preceding = preceding;
                element.preorder_descendant_postorder_ancestor = ancestor;
            }
        }
    }
}

pub enum TraversalType {
    PreRevPre,
    PostRevPost,
    PostRevPre,
    PreRevPost,
    AllTraversals,
}

pub struct StringStructAlgorithm;

impl LowerBoundMethod for StringStructAlgorithm {
    const NAME: &'static str = "SED-STRUCT";
    const SUPPORTS_INDEX: bool = false;

    type PreprocessedDataType = StringStructEDIndex;
    type PreprocessParams = TraversalType;
    type IndexType = ();
    type IndexParams = ();

    fn preprocess(
        &mut self,
        data: &[ParsedTree],
        params: Self::PreprocessParams,
    ) -> Result<Vec<Self::PreprocessedDataType>, String> {
        Ok(data
            .iter()
            .map(|t| preprocess_tree(t, &params))
            .collect::<Vec<_>>())
    }

    fn lower_bound(
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize {
        sed_struct_k(query, data, threshold)
    }

    fn build_index(
        _data: &[Self::PreprocessedDataType],
        _params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String> {
        Err("Indexing not supported for SED-STRUCT".to_string())
    }

    fn query_index(
        _query: &Self::PreprocessedDataType,
        _index: &Self::IndexType,
        _threshold: usize,
    ) -> Vec<usize> {
        vec![]
    }
}

fn preprocess_tree(tree: &ParsedTree, traversals: &TraversalType) -> StringStructEDIndex {
    let mut index = StringStructEDIndex::new(traversals, tree.count());
    let Some(root) = tree.iter().next() else {
        panic!("Unable to get root but tree is not empty!");
    };
    let root_id = tree.get_node_id(root).unwrap();

    let mut postorder_id = 0usize;
    let mut preorder_id = 0usize;
    let mut depth = 0usize;
    traverse_with_info(
        root_id,
        tree,
        &mut index,
        &mut postorder_id,
        &mut preorder_id,
        &mut depth,
    );

    index.reverse_if_needed();

    index
}

fn traverse_with_info(
    nid: NodeId,
    tree: &ParsedTree,
    index: &mut StringStructEDIndex,
    postorder_id: &mut usize,
    preorder_id: &mut usize,
    depth: &mut usize,
) -> usize {
    let mut subtree_size = 1;
    *depth += 1;
    // i am here at the current root
    let label = tree.get(nid).unwrap().get();
    index.push_pre_data(TraversalCharacter {
        char: *label,
        preorder_following_postorder_preceding: 0,
        preorder_descendant_postorder_ancestor: 0,
    });

    let pre_idx = index.get_pre_len() - 1;

    for cnid in nid.children(tree) {
        subtree_size += traverse_with_info(cnid, tree, index, postorder_id, preorder_id, depth);
    }

    *depth -= 1;
    *postorder_id += 1;
    *preorder_id += 1;

    // preceding
    let preceding = *postorder_id - subtree_size;
    let following = tree.count() - (*postorder_id + *depth);

    index.push_post_data(TraversalCharacter {
        char: *label,
        preorder_following_postorder_preceding: following as i32,
        preorder_descendant_postorder_ancestor: *depth as i32,
    });

    index.set_pre_data(
        pre_idx,
        following as i32,
        subtree_size as i32 - 1,
        preceding as i32,
        *depth as i32,
    );

    subtree_size
}

#[macro_export]
macro_rules! compute_trees {
    ($t1a:ident, $t2a:ident, $t1b:ident, $t2b:ident, $k:ident) => {{
        let res1 = bounded_string_edit_distance_with_structure($t1a, $t2a, $k);
        if res1 > $k {
            return res1;
        }
        let res2 = bounded_string_edit_distance_with_structure($t1b, $t2b, $k);
        std::cmp::max(res1, res2)
    }};
}

#[macro_export]
macro_rules! match_and_compute {
  (
    $t1:ident,
    $t2:ident,
    $k:ident,
    $(
      $variant_name:ident: [$field1:ident, $field2:ident]
    ),* $(,)?
  ) => {
    match ($t1, $t2) {
      $(
        (
          StringStructEDIndex::$variant_name {
            $field1: t1a,
            $field2: t1b,
            ..
          },
          StringStructEDIndex::$variant_name {
            $field1: t2a,
            $field2: t2b,
            ..
          },
        ) => {
          let res1 = bounded_string_edit_distance_with_structure(&t1a, &t2a, $k);
          if res1 > $k {
            return res1;
          }
          let res2 = bounded_string_edit_distance_with_structure(&t1b, &t2b, $k);
          std::cmp::max(res1, res2)
        }
      )*
      _ => {
        panic!("Both trees must be of the same traversal type!");
      }
    }
  };
}

/// Computes bounded string edit distance with known maximal threshold.
/// Returns distance at max of K. Algorithm by Hal Berghel and David Roach
/// Assumes that the trees indexes are of the same variant or panics.
fn sed_struct_k(t1: &StringStructEDIndex, t2: &StringStructEDIndex, k: usize) -> usize {
    let (mut t1, mut t2) = (t1, t2);
    if t1.get_size().abs_diff(t2.get_size()) > k {
        return k + 1;
    }

    if t1.get_size() > t2.get_size() {
        (t1, t2) = (t2, t1);
    }

    match_and_compute! {
      t1,
      t2,
      k,
      PreRevPre: [preorder, reversed_preorder],
      PostRevPost: [postorder, reversed_postorder],
      PostRevPre: [postorder, reversed_preorder],
      PreRevPost: [preorder, reversed_postorder],
      AllTraversals: [preorder, reversed_preorder],
    }
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
    // TODO: Handle cases, where the threshold k is bigger than both s1 and s2 lengths
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
    let zero_k: i32 = ((if s1len < threshold { s1len } else { threshold }) >> 1) + 2;

    // Calculate array length needed to store diagonal values
    let arr_len = size_diff + (zero_k) * 2 + 2;

    // Instead of storing the full DP matrix, Ukkonen's algorithm only stores
    // the current and next row (optimization described in the paper)
    let mut current_row = vec![(-1i32, true); arr_len as usize];
    let mut next_row = vec![(-1i32, true); arr_len as usize];
    let mut i = 0;
    // condition_diagonal is the diaogonal on which the resulting SED lies.
    // we will be checking this diagonal to determine if we can stop early
    let condition_diagonal = size_diff + zero_k;
    let end_max = condition_diagonal << 1;

    // prepare a simple test function if characters are eligible for substitution
    #[inline(always)]
    fn struct_diff(t1: &TraversalCharacter, t2: &TraversalCharacter) -> i32 {
        (t1.preorder_following_postorder_preceding
            .abs_diff(t2.preorder_following_postorder_preceding)
            + t1.preorder_descendant_postorder_ancestor
                .abs_diff(t2.preorder_descendant_postorder_ancestor)) as i32
    }

    let mut next_allowed_substitution = true;
    loop {
        // i here is the current allowed edit distance
        i += 1;
        std::mem::swap(&mut next_row, &mut current_row);

        let start: i32;
        let mut next_cell: i32;
        let mut previous_cell: i32;
        let mut current_cell: i32 = -1;

        // Calculate the starting diagonal for this iteration
        // This follows Berghel & Roach's band algorithm approach
        if i <= zero_k {
            start = -i + 1;
            next_cell = i - 2i32;
        } else {
            // 2 if i = 11 and zero_k = 10
            start = i - (zero_k << 1) + 1;
            unsafe {
                (next_cell, next_allowed_substitution) =
                    *current_row.get_unchecked((zero_k + start) as usize);
            }
        }

        // Calculate the ending diagonal for this iteration
        let end: i32;
        if i <= condition_diagonal {
            end = i;
            unsafe {
                *next_row.get_unchecked_mut((zero_k + i) as usize) = (-1, true);
            }
        } else {
            end = end_max - i;
        }
        // let current_edit_distance = (i - 1) as u32;
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
                max_row_number = max(current_cell + 1, max(previous_cell, next_cell + 1));

                // If substitution is not allowed, treat as insertion/deletion (not diagonal move)
                if !can_substitute {
                    // pokud nemuzu delat substituci a previous a next nedaji vetsi cislo, tak jen vezmu cislo
                    // current_cell, rovnou zapisu a nemusim se ani pokouset delat extension - zda se mi to zvetsi

                    max_row_number = max(max(previous_cell, current_cell), next_cell + 1);

                    if max_row_number == current_cell {
                        // TODO: jen zapsat a continue
                        *next_row.get_unchecked_mut(diagonal_index) = (max_row_number, false);
                        diagonal_index += 1;
                        continue;
                    }
                }
            }
            // can_substitute = true;
            // let mut max_row_number = max_row_number as usize;
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

                while max_row_number < s1len && (max_row_number + diag_offset) < s2len {
                    let char_eq = s1.get_unchecked((max_row_number) as usize).char
                        == s2
                            .get_unchecked((max_row_number + diag_offset) as usize)
                            .char;

                    // Check structural constraints
                    // this must always be evaluated because struct_ok is initialized to false
                    struct_ok = (allowed_edits
                        + struct_diff(
                            s1.get_unchecked((max_row_number) as usize),
                            s2.get_unchecked((max_row_number + diag_offset) as usize),
                        ))
                        <= k;

                    if !char_eq || !struct_ok {
                        break;
                    }

                    max_row_number += 1;
                }

                // Branchless update: advance by the minimum of character and structural constraints

                // disable substitution if we hit the big sturctural diff. If the problem is only character mismatch, it should be true
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
            if !(next_row.get_unchecked(condition_diagonal as usize).0 < s1len as i32
                && i <= threshold)
            {
                if threshold < k as i32 {
                    break ((i - 1) + k as i32 - threshold) as usize;
                }

                if !(next_row.get_unchecked(condition_diagonal as usize).0 >= s1len as i32)
                    && i > threshold
                {
                    break usize::MAX;
                }

                break (i - 1) as usize;
            }
        }
    }
}

pub struct StringStructFactory;

impl AlgorithmFactory for StringStructFactory {
    fn create_algorithm(&self, algo_type: ted_base::AlgorithmType) -> impl LowerBoundMethod {
        match algo_type {
            ted_base::AlgorithmType::StringStruct => StringStructAlgorithm,
            _ => panic!("Unsupported algorithm type for SedFactory"),
        }
    }
}

#[cfg(test)]
mod tests {
    use tree_parsing::{parse_single, LabelDict};

    use super::*;

    #[test]
    fn test_bounded_sed_structure() {
        // i have simple alphabet mapping for testing purposes
        // 1 -> g
        // 2 -> a
        // 3 -> r
        // 4 -> v
        // 5 -> e
        // 6 -> y

        // arvey
        let v1 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 2,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 3,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 4,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 5,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 6,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];
        // avery
        let v2 = vec![
            TraversalCharacter {
                char: 2,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 3,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 4,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 5,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 5,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 3);
        assert_eq!(result, 2);
    }

    #[test]
    fn test_bounded_sed_structure_2() {
        // i have simple alphabet mapping for testing purposes
        // 1 -> s
        // 2 -> k
        // 3 -> i
        // 4 -> t
        // 5 -> e
        // 6 -> n
        // 7 -> g

        // sitting
        let v1 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 3,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 4,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 4,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 3,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 6,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 7,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];
        // kitten
        let v2 = vec![
            TraversalCharacter {
                char: 2,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 3,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 4,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 4,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 5,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 6,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 3);
        assert_eq!(result, 3);
    }

    #[test]
    fn test_bounded_sed_structure_simple() {
        // i have simple alphabet mapping for testing purposes
        // 1 -> a
        // 2 -> b

        let v1 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 4,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];
        let v2 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 1,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 1);
        assert_eq!(result, usize::MAX);
    }

    #[test]
    fn test_bounded_sed_structure_simple_unmatched() {
        // i have simple alphabet mapping for testing purposes
        // 1 -> a
        // 2 -> b

        let v1 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 2,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 3,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 2,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];
        let v2 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 2,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];
        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 2);
        assert_eq!(result, 2);
    }

    #[test]
    fn test_bounded_sed_structure_simple_test() {
        // i have simple alphabet mapping for testing purposes
        // 1 -> a
        // 2 -> b

        let v1 = vec![
            TraversalCharacter {
                char: 2,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 2,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];
        let v2 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v2, &v1, 1);
        assert_eq!(result, usize::MAX);
    }

    #[test]
    fn test_sed_simple() {
        let v1 = vec![
            TraversalCharacter {
                char: 1,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 2,
                preorder_descendant_postorder_ancestor: 0,
                preorder_following_postorder_preceding: 0,
            },
        ];
        let v2 = vec![
            TraversalCharacter {
                char: 1,
                preorder_descendant_postorder_ancestor: 0,
                preorder_following_postorder_preceding: 0,
            },
            TraversalCharacter {
                char: 2,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
            TraversalCharacter {
                char: 3,
                preorder_following_postorder_preceding: 0,
                preorder_descendant_postorder_ancestor: 0,
            },
        ];

        let result = bounded_string_edit_distance_with_structure(&v1, &v2, 2);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_sed_preorder_structure() {
        let t1str = "{a{a{b{a{a}}}}}".to_owned();
        let t2str = "{a{b{b{b}}{a{a}}}}".to_owned();
        let mut ld = LabelDict::new();
        let qt = parse_single(t1str, &mut ld);
        let tt = parse_single(t2str, &mut ld);
        let result = StringStructAlgorithm
            .preprocess(&[qt.clone(), tt.clone()], TraversalType::AllTraversals)
            .unwrap();
        let [qs, ts, ..] = result.as_slice() else {
            panic!("Expected at least 2 elements");
        };

        assert_eq!(
            match qs {
                StringStructEDIndex::AllTraversals { preorder, .. } => preorder.clone(),
                _ => panic!("Expected AllTraversals variant"),
            },
            vec![
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 4,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 3,
                },
                TraversalCharacter {
                    char: 2,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 2,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 1,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 0,
                },
            ]
        );

        assert_eq!(
            match qs {
                StringStructEDIndex::AllTraversals { postorder, .. } => postorder.clone(),
                _ => panic!("Expected AllTraversals variant"),
            },
            vec![
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 4,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 3,
                },
                TraversalCharacter {
                    char: 2,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 2,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 1,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 0,
                },
            ]
        );

        assert_eq!(
            match ts {
                StringStructEDIndex::AllTraversals { preorder, .. } => preorder.clone(),
                _ => panic!("Expected AllTraversals variant"),
            },
            vec![
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 5,
                },
                TraversalCharacter {
                    char: 2,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 4,
                },
                TraversalCharacter {
                    char: 2,
                    preorder_following_postorder_preceding: 2,
                    preorder_descendant_postorder_ancestor: 1,
                },
                TraversalCharacter {
                    char: 2,
                    preorder_following_postorder_preceding: 2,
                    preorder_descendant_postorder_ancestor: 0,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 1,
                },
                TraversalCharacter {
                    char: 1,
                    preorder_following_postorder_preceding: 0,
                    preorder_descendant_postorder_ancestor: 0,
                },
            ]
        );
    }
}
