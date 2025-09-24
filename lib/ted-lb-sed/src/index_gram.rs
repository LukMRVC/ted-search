use itertools::Itertools;
use rustc_hash::FxHashMap;
use tree_parsing::LabelId;

use crate::Traversal;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
struct QSig {
    sig: Vec<LabelId>,
    pos: i32,
}

#[derive(Debug)]
pub struct IndexGram {
    q: usize,
    inv_index: FxHashMap<Vec<LabelId>, Vec<(usize, i32, i32)>>,
}

impl IndexGram {
    pub const EMPTY_VALUE: i32 = i32::MAX;

    pub fn new(data: &[Traversal], q: usize) -> Self {
        let mut inv_index = FxHashMap::default();

        for (sid, mut sdata) in data.iter().cloned().enumerate() {
            let sig_size = sdata.len().div_ceil(q);
            let orig_len = sdata.len() as i32;
            sdata.append(&mut vec![Self::EMPTY_VALUE; sig_size * q - sdata.len()]);

            sdata.windows(q).enumerate().for_each(|(i, w)| {
                inv_index
                    .entry(w.to_vec())
                    .and_modify(|postings: &mut Vec<(usize, i32, i32)>| {
                        postings.push((sid, orig_len, i as i32))
                    })
                    .or_insert(vec![(sid, orig_len, i as i32)]);
            });
        }

        IndexGram {
            q,
            // q_grams,
            inv_index,
        }
    }

    pub fn query(&self, mut query: Vec<i32>, k: usize) -> Result<Vec<usize>, String> {
        let sig_size = query.len().div_ceil(self.q);
        let min_allowed_sig_size = query.len() / self.q;
        if k >= min_allowed_sig_size {
            return Err("Query is too small for that threshold!".to_owned());
        }
        let min_match_size = query.len().saturating_sub(k) as i32;
        let max_match_size = (query.len() + k + 1) as i32;
        query.append(&mut vec![
            Self::EMPTY_VALUE;
            sig_size * self.q - query.len()
        ]);

        let chunks: Vec<QSig> = query
            .chunks(self.q)
            .enumerate()
            .map(|(pos, c)| QSig {
                sig: c.to_vec(),
                pos: (pos * self.q) as i32,
            })
            .collect();
        let mut cs = FxHashMap::default();

        // for chunk in chunks.iter().take(k + 1)
        for chunk in chunks.iter() {
            // dbg!(chunk);
            if let Some(postings) = self.inv_index.get(&chunk.sig) {
                let Err(start) = postings.binary_search_by(|probe| {
                    probe
                        .1
                        .cmp(&min_match_size)
                        .then(std::cmp::Ordering::Greater)
                }) else {
                    panic!("Binary search cannot result in Ok!");
                };
                let Err(end) = postings.binary_search_by(|probe| {
                    probe
                        .1
                        .cmp(&max_match_size)
                        .then(std::cmp::Ordering::Greater)
                }) else {
                    panic!("Binary search cannot result in Ok!");
                };
                let to_take = end - start;
                for (cid, _, gram_pos) in postings.iter().skip(start).take(to_take) {
                    if chunk.pos.abs_diff(*gram_pos) <= (k as u32) {
                        cs.entry(*cid)
                            .and_modify(|candidate_grams: &mut Vec<(&QSig, i32)>| {
                                candidate_grams.push((chunk, *gram_pos))
                            })
                            .or_insert(vec![(chunk, *gram_pos)]);
                        // cs.insert(*cid);
                    }
                }
            }
        }

        let lb: usize = sig_size - k;
        let mut opt = vec![0; 128];
        // count and true matches filter
        let candidates = cs
            .into_iter()
            .filter_map(|(cid, mut candidate_gram_matches)| {
                if candidate_gram_matches.len() < lb {
                    return None;
                }
                candidate_gram_matches.sort_by_key(|(chunk, _)| chunk.pos);

                // true match filter
                let omni_match = QSig {
                    sig: vec![-1],
                    pos: i32::MAX,
                };
                candidate_gram_matches.insert(0, (&omni_match, omni_match.pos));
                // let mut opt = vec![0; candidate_gram_matches.len()];
                opt.fill(0);

                if opt.len() < candidate_gram_matches.len() {
                    opt.resize(candidate_gram_matches.len(), 0);
                }

                #[inline(always)]
                fn compatible(m1: &(&QSig, i32), m2: &(&QSig, i32), n: i32) -> bool {
                    *unsafe { m2.0.sig.get_unchecked(0) } == -1
                        || ((m1.0.pos != m2.0.pos && m1.0.sig != m2.0.sig) && m1.1 >= m2.1 + n)
                }

                let qsize = self.q as i32;
                unsafe {
                    // the first in tuple is the q-chunk of query, second is q-gram of data string

                    let mut total_max = i32::MIN;
                    for kc in 1..candidate_gram_matches.len() {
                        let mut mx = i32::MIN;
                        let mn = std::cmp::min(kc, candidate_gram_matches.len() - lb + 1);
                        for i in 1..=mn {
                            if *opt.get_unchecked(kc - i) > mx
                                && compatible(
                                    candidate_gram_matches.get_unchecked(kc),
                                    candidate_gram_matches.get_unchecked(kc - i),
                                    qsize,
                                )
                            {
                                mx = opt.get_unchecked(kc - i) + 1;
                            }
                        }
                        *opt.get_unchecked_mut(kc) = mx;
                        total_max = std::cmp::max(total_max, mx);
                        if kc >= lb && total_max >= lb as i32 {
                            return Some(cid);
                        }
                    }
                }
                if opt.iter().skip(lb).max().unwrap() >= &(lb as i32) {
                    return Some(cid);
                }
                None
            })
            // .filter(|cid| self.count_filter(*cid, sig_size, k, &chunks))
            .collect_vec();

        Ok(candidates)
    }
}
