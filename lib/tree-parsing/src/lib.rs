mod error;
use crossbeam_channel::Sender;
use indextree::{Arena, NodeEdge, NodeId};
use itertools::Itertools;
use memchr::memchr2_iter;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::string::String;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::error::{DatasetParseError, TreeParseError};

const TOKEN_START: u8 = b'{';
const TOKEN_END: u8 = b'}';
const ESCAPE_CHAR: u8 = b'\\';

pub type LabelId = i32;
pub type LabelDict = HashMap<String, (LabelId, usize)>;
pub type ParsedTree = Arena<LabelId>;

macro_rules! buf_open_file {
    ($file_path:ident) => {
        BufReader::new(File::open($file_path)?)
    };
}

fn braces_parity_check(parity: &mut i32, addorsub: i32) -> Result<(), TreeParseError> {
    *parity += addorsub;
    if *parity < 0 {
        return Err(TreeParseError::IncorrectFormat(
            "Parity of brces does not match".to_owned(),
        ));
    }
    Ok(())
}

pub fn tree_to_bracket(tree: &ParsedTree) -> String {
    let mut bracket_notation = String::with_capacity(tree.count() * 4);
    let Some(root) = tree.iter().next() else {
        panic!("Root not found!");
    };
    let root_id = tree.get_node_id(root).expect("Root ID not found!");

    for edge in root_id.traverse(tree) {
        match edge {
            NodeEdge::Start(node_id) => {
                bracket_notation.push('{');
                bracket_notation.push_str(&tree.get(node_id).unwrap().get().to_string());
            }
            NodeEdge::End(_) => {
                bracket_notation.push('}');
            }
        }
    }

    bracket_notation
}

#[inline(always)]
fn is_escaped(byte_string: &[u8], offset: usize) -> bool {
    offset > 0 && byte_string[offset - 1] == ESCAPE_CHAR
}

pub fn parse_dataset(
    dataset_file: &impl AsRef<Path>,
    label_dict: &mut LabelDict,
) -> Result<Vec<ParsedTree>, DatasetParseError> {
    // Use scc::HashMap for lock-free concurrent label dictionary during parsing
    let scc_label_dict = scc::HashMap::new();

    // Initialize scc::HashMap with existing label_dict entries and find max ID
    let max_node_id = AtomicI32::new(label_dict.values().map(|(id, _)| *id).max().unwrap_or(0));

    for (label, (id, count)) in label_dict.iter() {
        let _ = scc_label_dict.insert_sync(label.clone(), (*id, *count));
    }

    let reader = BufReader::new(File::open(dataset_file).unwrap());

    // Parse in parallel while tracking original index for stable ordering
    // enumerate() is lazy, par_bridge() streams directly from the reader
    let mut trees: Vec<(usize, ParsedTree)> = reader
        .lines()
        .enumerate()
        .par_bridge()
        .filter_map(|(idx, tree_line)| {
            let tree_line = tree_line.ok()?;
            if !tree_line.is_ascii() {
                return None;
            }

            parse_tree_directly(&tree_line, &scc_label_dict, &max_node_id)
                .ok()
                .map(|tree| (idx, tree))
        })
        .collect();

    // Convert scc::HashMap to FxHashMap using scan_sync
    label_dict.clear();
    scc_label_dict.retain_sync(|label, (id, count)| {
        label_dict.insert(label.clone(), (*id, *count));
        false
    });

    // Stable sort: by tree count, then by original index as tiebreaker
    trees.sort_by(|(idx_a, a), (idx_b, b)| a.count().cmp(&b.count()).then(idx_a.cmp(idx_b)));

    // Extract just the trees, discarding indices
    let trees = trees.into_iter().map(|(_, tree)| tree).collect();

    Ok(trees)
}

pub fn parse_queries(
    query_file: &impl AsRef<Path>,
    ld: &mut LabelDict,
    delimiter: char,
) -> Result<Vec<(usize, ParsedTree)>, DatasetParseError> {
    let reader = buf_open_file!(query_file);
    let trees: Vec<(usize, Vec<String>)> = reader
        .lines()
        .filter_map(|l| {
            let l = l.expect("line reading failed!");
            let (threshold_str, tree) = l.split_once(delimiter)?;
            Some((threshold_str.parse::<usize>().unwrap(), tree.to_string()))
        })
        .filter_map(|(t, tree)| {
            let tokens = parse_tree_tokens(tree, None);
            if tokens.is_err() {
                return None;
            }
            let tks: Vec<String> = tokens
                .unwrap()
                .iter()
                .map(|tkn| tkn.to_string())
                .collect_vec();

            Some((t, tks))
        })
        .collect::<Vec<_>>();

    let only_tokens = trees
        .iter()
        .map(|(_, tkns)| tkns.iter().map(|t| t.as_str()).collect_vec())
        .collect_vec();

    update_label_dict(&only_tokens, ld);
    let trees = trees
        .iter()
        .filter_map(|(t, tokens)| {
            let parsed_tree = parse_tree(tokens, ld);
            if parsed_tree.is_err() {
                return None;
            }

            Some((*t, parsed_tree.unwrap()))
        })
        .collect();

    Ok(trees)
}

pub fn parse_single(tree_str: String, label_dict: &mut LabelDict) -> ParsedTree {
    if !tree_str.is_ascii() {
        panic!("Passed tree string is not ASCII");
    }

    let tokens = parse_tree_tokens(tree_str, None).expect("Failed to parse single tree");
    let str_tokens = tokens.iter().map(|t| t.as_str()).collect_vec();
    let token_col = vec![str_tokens];
    update_label_dict(&token_col, label_dict);
    parse_tree(&tokens, label_dict).unwrap()
}

pub fn update_label_dict(tokens_collection: &[Vec<&str>], ld: &mut LabelDict) {
    let labels_only = tokens_collection
        .par_iter()
        .flat_map(|tree_tokens| {
            tree_tokens
                .iter()
                .filter(|token| **token != "{" && **token != "}")
                .map(|label_token| label_token.to_string())
                .collect_vec()
        })
        .collect::<Vec<_>>();

    let mut max_node_id = ld.values().len() as LabelId;
    for lbl in labels_only {
        ld.entry(lbl)
            .and_modify(|(_, lblcnt)| *lblcnt += 1)
            .or_insert_with(|| {
                max_node_id += 1;
                (max_node_id, 1)
            });
    }
}

// Fused parsing: tokenize and build tree directly without intermediate Vec<String>
// Eliminates brace tokens entirely - only stores label IDs in the Arena
fn parse_tree_directly(
    tree_line: &str,
    label_dict: &scc::HashMap<String, (LabelId, usize)>,
    max_node_id: &AtomicI32,
) -> Result<ParsedTree, TreeParseError> {
    use TreeParseError as TPE;

    let tree_bytes = tree_line.as_bytes();
    let token_positions: Vec<usize> = memchr2_iter(TOKEN_START, TOKEN_END, tree_bytes)
        .filter(|char_pos| !is_escaped(tree_bytes, *char_pos))
        .collect();

    if token_positions.len() < 2 {
        return Err(TPE::IncorrectFormat(
            "Minimal of 2 brackets not found!".to_owned(),
        ));
    }

    // Estimate tree size: roughly half of tokens are labels (the other half are braces)
    let estimated_nodes = token_positions.len() / 2;
    let mut tree_arena = ParsedTree::with_capacity(estimated_nodes);
    let mut node_stack: Vec<NodeId> = Vec::with_capacity(32); // typical tree depth
    let mut parity_check = 0;

    let mut token_iterator = token_positions.iter().peekable();

    while let Some(token_pos) = token_iterator.next() {
        match tree_bytes[*token_pos] {
            TOKEN_START => {
                braces_parity_check(&mut parity_check, 1)?;

                let Some(token_end) = token_iterator.peek() else {
                    let err_msg = format!("Label has no ending token near col {token_pos}");
                    return Err(TPE::IncorrectFormat(err_msg));
                };

                // Extract label bytes without allocating String for braces
                let label_bytes = &tree_bytes[(token_pos + 1)..**token_end];

                // Skip empty labels or escaped braces that result in brace-only labels
                if label_bytes.is_empty() || label_bytes == b"{" || label_bytes == b"}" {
                    continue;
                }

                // Convert to string only for the label lookup/insert
                let label = unsafe { String::from_utf8_unchecked(label_bytes.to_vec()) };

                // Get or insert label ID from concurrent hashmap
                let label_id = {
                    let entry = label_dict.entry_sync(label);
                    match entry {
                        scc::hash_map::Entry::Occupied(mut occ) => {
                            let (id, count) = occ.get_mut();
                            *count += 1;
                            *id
                        }
                        scc::hash_map::Entry::Vacant(vac) => {
                            let new_id = max_node_id.fetch_add(1, Ordering::Relaxed) + 1;
                            vac.insert_entry((new_id, 1));
                            new_id
                        }
                    }
                };

                // Create node and append to tree
                let node = tree_arena.new_node(label_id);
                if let Some(parent) = node_stack.last() {
                    parent.append(node, &mut tree_arena);
                } else if tree_arena.count() > 1 {
                    return Err(TPE::IncorrectFormat(
                        "Multiple root nodes detected".to_owned(),
                    ));
                }
                node_stack.push(node);
            }
            TOKEN_END => {
                braces_parity_check(&mut parity_check, -1)?;
                if node_stack.pop().is_none() {
                    return Err(TPE::IncorrectFormat("Wrong bracket pairing".to_owned()));
                }
            }
            _ => return Err(TPE::TokenizerError),
        }
    }

    if parity_check != 0 {
        return Err(TPE::IncorrectFormat("Unbalanced brackets".to_owned()));
    }

    Ok(tree_arena)
}

pub fn parse_tree(tokens: &[String], ld: &LabelDict) -> Result<ParsedTree, TreeParseError> {
    let mut tree_arena = ParsedTree::with_capacity(tokens.len() / 2);
    let mut node_stack: Vec<NodeId> = vec![];

    for t in tokens.iter().skip(1) {
        match t.as_str() {
            "{" => continue,
            "}" => {
                let Some(_) = node_stack.pop() else {
                    return Err(TreeParseError::IncorrectFormat(
                        "Wrong bracket pairing".to_owned(),
                    ));
                };
            }
            label_str => {
                let Some((label, _)) = ld.get(label_str) else {
                    return Err(TreeParseError::TokenizerError);
                };
                let n = tree_arena.new_node(*label);
                if let Some(last_node) = node_stack.last() {
                    last_node.append(n, &mut tree_arena);
                } else if tree_arena.count() > 1 {
                    return Err(TreeParseError::IncorrectFormat(
                        "Reached unexpected end of token".to_owned(),
                    ));
                };
                node_stack.push(n);
            }
        }
    }
    Ok(tree_arena)
}

fn parse_tree_tokens(
    tree_bytes: String,
    sender_channel: Option<&mut Sender<String>>,
) -> Result<Vec<String>, TreeParseError> {
    use TreeParseError as TPE;

    let tree_bytes = tree_bytes.as_bytes();
    let token_positions: Vec<usize> = memchr2_iter(TOKEN_START, TOKEN_END, tree_bytes)
        .filter(|char_pos| !is_escaped(tree_bytes, *char_pos))
        .collect();

    if token_positions.len() < 2 {
        return Err(TPE::IncorrectFormat(
            "Minimal of 2 brackets not found!".to_owned(),
        ));
    }

    let mut str_tokens = vec![];
    let mut parity_check = 0;

    let mut token_iterator = token_positions.iter().peekable();

    while let Some(token_pos) = token_iterator.next() {
        match tree_bytes[*token_pos] {
            TOKEN_START => {
                braces_parity_check(&mut parity_check, 1)?;
                unsafe {
                    str_tokens.push(String::from_utf8_unchecked(
                        tree_bytes[*token_pos..(token_pos + 1)].to_vec(),
                    ));
                }
                let Some(token_end) = token_iterator.peek() else {
                    let err_msg = format!("Label has no ending token near col {token_pos}");
                    return Err(TPE::IncorrectFormat(err_msg));
                };
                let label = unsafe {
                    String::from_utf8_unchecked(tree_bytes[(token_pos + 1)..**token_end].to_vec())
                };
                str_tokens.push(label.clone());
                if let Some(ref s) = sender_channel {
                    s.send(label).expect("Failed sending label");
                }
            }
            TOKEN_END => {
                braces_parity_check(&mut parity_check, -1)?;
                let label = unsafe {
                    String::from_utf8_unchecked(tree_bytes[*token_pos..(token_pos + 1)].to_vec())
                };
                str_tokens.push(label.clone());
                if let Some(ref s) = sender_channel {
                    s.send(label).expect("Failed sending label");
                }
            }
            _ => return Err(TPE::TokenizerError),
        }
    }
    Ok(str_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_into_tokens() {
        let input = "{NP{NP{NNS{Fees}}}{QP{CD{1}}{CD{3\\}/4}}}{Interpunction{.}}}".to_owned();
        let tokens = parse_tree_tokens(input, None);
        assert!(tokens.is_ok());
        let tokens = tokens.unwrap();
        assert_eq!(tokens.len(), 33);
        assert_eq!(
            tokens,
            vec![
                "{",
                "NP",
                "{",
                "NP",
                "{",
                "NNS",
                "{",
                "Fees",
                "}",
                "}",
                "}",
                "{",
                "QP",
                "{",
                "CD",
                "{",
                "1",
                "}",
                "}",
                "{",
                "CD",
                "{",
                "3\\}/4",
                "}",
                "}",
                "}",
                "{",
                "Interpunction",
                "{",
                ".",
                "}",
                "}",
                "}"
            ]
        );
    }

    #[test]
    fn test_parses_into_tokens_2() {
        let input = "{einsteinstrasse{1}{3}}".to_owned();
        let tokens = parse_tree_tokens(input, None);
        assert!(tokens.is_ok());
        let tokens = tokens.unwrap();
        assert_eq!(
            tokens,
            vec!["{", "einsteinstrasse", "{", "1", "}", "{", "3", "}", "}"]
        );
    }

    #[test]
    fn test_parses_escaped() {
        use std::string::String;
        let input = String::from(r#"{article{key{An optimization of \log data}}}"#);
        let tokens = parse_tree_tokens(input, None);
        assert!(tokens.is_ok());
        let tokens = tokens.unwrap();
        assert_eq!(
            tokens,
            vec![
                "{",
                "article",
                "{",
                "key",
                "{",
                r"An optimization of \log data",
                "}",
                "}",
                "}"
            ]
        );
    }

    #[test]
    fn test_parses_into_tree_arena() {
        let input = "{einsteinstrasse{1}{3}}".to_owned();
        let tokens = parse_tree_tokens(input, None);
        let tokens = tokens.unwrap();
        let ld = LabelDict::from([
            ("einsteinstrasse".to_owned(), (1, 1)),
            ("1".to_owned(), (2, 1)),
            ("3".to_owned(), (3, 1)),
        ]);
        let tree_arena = parse_tree(&tokens, &ld).unwrap();
        let mut arena = ParsedTree::new();

        let n1 = arena.new_node(1);
        let n2 = arena.new_node(2);
        let n3 = arena.new_node(3);
        n1.append(n2, &mut arena);
        n1.append(n3, &mut arena);

        assert_eq!(tree_arena, arena);
    }

    #[test]
    fn test_updated_label_dict() {
        let input = "{einsteinstrasse{1}{3}}".to_owned();
        let tokens = parse_tree_tokens(input, None);
        let tokens = tokens
            .unwrap()
            .into_iter()
            .map(|s| s.to_string())
            .collect_vec();
        let input2 = "{weinsteinstrasse{3}{2}}".to_owned();
        let tokens2 = parse_tree_tokens(input2, None);
        let tokens2 = tokens2
            .unwrap()
            .into_iter()
            .map(|s| s.to_string())
            .collect_vec();
        let mut ld = LabelDict::default();
        let token_col: Vec<Vec<&str>> = vec![
            tokens.iter().map(|s| s.as_str()).collect(),
            tokens2.iter().map(|s| s.as_str()).collect(),
        ];
        update_label_dict(&token_col, &mut ld);

        let tld = LabelDict::from([
            ("einsteinstrasse".to_owned(), (1, 1)),
            ("1".to_owned(), (2, 1)),
            ("3".to_owned(), (3, 2)),
            ("weinsteinstrasse".to_owned(), (4, 1)),
            ("2".to_owned(), (5, 1)),
        ]);
        assert_eq!(ld, tld, "Label dicts are equal");
    }
}
