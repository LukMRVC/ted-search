pub use ted_base::{AlgorithmFactory, AlgorithmType, LowerBoundMethod, TraversalKind};
use ted_lb_bib::BinaryBranchAlgorithm;
pub use ted_lb_bib::BinaryBranchFactory;
use ted_lb_label_intersection::LabelIntersectionAlgorithm;
pub use ted_lb_label_intersection::LabelIntersectionFactory;
use ted_lb_sed::SedAlgorithm;
pub use ted_lb_sed::SedFactory;
use ted_lb_sed_exact::SedExactAlgorithm;
pub use ted_lb_sed_exact::SedExactFactory;
use ted_lb_sed_struct::StringStructAlgorithm;
pub use ted_lb_sed_struct::StringStructFactory;
pub use ted_lb_structural::{StructuralAlgorithm, StructuralFactory};
pub use tree_parsing::{
    parse_dataset, parse_queries, parse_single, update_label_dict, LabelDict, LabelId, ParsedTree,
};

pub enum Algorithm {
    LabelIntersection(LabelIntersectionAlgorithm),
    Sed(SedAlgorithm),
    SedExact(SedExactAlgorithm),
    StringStruct(StringStructAlgorithm),
    Structural(StructuralAlgorithm),
    BinaryBranch(BinaryBranchAlgorithm),
}

impl From<SedAlgorithm> for Algorithm {
    fn from(algo: SedAlgorithm) -> Self {
        Algorithm::Sed(algo)
    }
}

impl From<StringStructAlgorithm> for Algorithm {
    fn from(algo: StringStructAlgorithm) -> Self {
        Algorithm::StringStruct(algo)
    }
}

impl From<SedExactAlgorithm> for Algorithm {
    fn from(algo: SedExactAlgorithm) -> Self {
        Algorithm::SedExact(algo)
    }
}

impl From<StructuralAlgorithm> for Algorithm {
    fn from(algo: StructuralAlgorithm) -> Self {
        Algorithm::Structural(algo)
    }
}

impl From<LabelIntersectionAlgorithm> for Algorithm {
    fn from(algo: LabelIntersectionAlgorithm) -> Self {
        Algorithm::LabelIntersection(algo)
    }
}

impl From<BinaryBranchAlgorithm> for Algorithm {
    fn from(algo: BinaryBranchAlgorithm) -> Self {
        Algorithm::BinaryBranch(algo)
    }
}

fn run_search_pipeline<T: LowerBoundMethod>(
    algo_instance: &T,
    data: &[ParsedTree],
    queries: &[(usize, ParsedTree)],
) -> Vec<Vec<usize>> {
    let preprocessed = algo_instance
        .preprocess(data)
        .expect("Unable to preprocess data");
    let preprocessed_queries = queries
        .iter()
        .map(|(k, query)| {
            let mut pq = algo_instance
                .preprocess(&[query.clone()])
                .expect("Unable to preprocess query");
            let pq: <T as LowerBoundMethod>::PreprocessedDataType = pq.remove(0);
            (k.to_owned(), pq)
        })
        .collect::<Vec<_>>();

    let mut results = Vec::new();

    for (k, query) in preprocessed_queries {
        let mut result = Vec::new();
        for (i, data_tree) in preprocessed.iter().enumerate() {
            let lb = algo_instance.lower_bound(&query, data_tree, k);
            if lb <= k * T::DIVISOR {
                result.push(i);
            }
        }
        results.push(result);
    }
    results
}

impl Algorithm {
    /// Returns the indices of the data trees that are within the given threshold of each query tree.
    pub fn search(&self, data: &[ParsedTree], queries: &[(usize, ParsedTree)]) -> Vec<Vec<usize>> {
        match self {
            Algorithm::LabelIntersection(algo) => run_search_pipeline(algo, data, queries),
            Algorithm::Sed(algo) => run_search_pipeline(algo, data, queries),
            Algorithm::SedExact(algo) => run_search_pipeline(algo, data, queries),
            Algorithm::StringStruct(algo) => run_search_pipeline(algo, data, queries),
            Algorithm::Structural(algo) => run_search_pipeline(algo, data, queries),
            Algorithm::BinaryBranch(algo) => run_search_pipeline(algo, data, queries),
        }
    }
}

pub fn create_algorithm<F: AlgorithmFactory>() -> Algorithm
where
    // We can now name the concrete type returned by the factory!
    Algorithm: From<F::AlgorithmType>,
{
    // The type of 'algo' is explicitly F::Algorithm
    let algo = F::create_algorithm();

    // The compiler can verify this conversion
    algo.into()
}

pub fn create_sed_algorithm(first: TraversalKind, second: TraversalKind) -> Algorithm {
    Algorithm::Sed(SedAlgorithm::new(first, second))
}

pub fn create_sed_exact_algorithm(first: TraversalKind, second: TraversalKind) -> Algorithm {
    Algorithm::SedExact(SedExactAlgorithm::new(first, second))
}

pub fn create_sed_struct_algorithm(first: TraversalKind, second: TraversalKind) -> Algorithm {
    Algorithm::StringStruct(StringStructAlgorithm::new(first, second))
}
