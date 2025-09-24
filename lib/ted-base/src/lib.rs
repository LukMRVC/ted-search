use tree_parsing::ParsedTree;

/// Trait for lower bound algorithm methods
pub trait LowerBoundMethod {
    /// Name/identifier for this method
    const NAME: &'static str;

    /// Constant indicating whether the method supports an index
    const SUPPORTS_INDEX: bool;

    /// Type of preprocessed data if needed
    /// If no preprocessing is needed, this can be `()`
    type PreprocessedDataType;

    /// Type of parameters for preprocessing if needed
    /// If no parameters are needed, this can be `()`
    type PreprocessParams;

    /// Type of index if supported
    /// If `SUPPORTS_INDEX` is false, this can be `()`
    type IndexType;

    /// Type of parameters for index construction if needed
    /// If no parameters are needed, this can be `()`
    type IndexParams;

    /// Preprocess the data before computing lower bound
    fn preprocess(
        &mut self,
        data: &[ParsedTree],
        params: Self::PreprocessParams,
    ) -> Result<Vec<Self::PreprocessedDataType>, String>;

    /// Compute the lower bound for 2 preprocessed trees
    /// and a given threshold
    fn lower_bound(
        query: &Self::PreprocessedDataType,
        data: &Self::PreprocessedDataType,
        threshold: usize,
    ) -> usize;

    fn build_index(
        data: &[Self::PreprocessedDataType],
        params: &Self::IndexParams,
    ) -> Result<Self::IndexType, String>;

    /// Query the index with the preprocessed query data
    /// and return a list of candidate indices
    fn query_index(
        query: &Self::PreprocessedDataType,
        index: &Self::IndexType,
        threshold: usize,
    ) -> Vec<usize>;
}

/// Enum to identify different algorithm types
#[derive(Debug, Clone, PartialEq)]
pub enum AlgorithmType {
    Sed,
    StringStruct,
    Structural,
    BinaryBranch,
    LabelIntersection,
    Histogram,
}

/// Factory trait for creating algorithm instances
pub trait AlgorithmFactory {
    fn create_algorithm(&self, algo_type: AlgorithmType) -> impl LowerBoundMethod;
}
