# Load datasets from datasets directory into variable
datasets_dir="./datasets"
datasets_dir="./datasets"

datasets=("$datasets_dir/sentiment" "$datasets_dir/rna" "$datasets_dir/labels-4" "$datasets_dir/high-fanout")

traversals_combinations=(
  "preorder,postorder"
  "reversed-postorder,preorder"
  "reversed-postorder,postorder"
  "reversed-preorder,preorder"
  "reversed-preorder,postorder"
  "reversed-postorder,reversed-preorder"
)

# Process each dataset
for dataset in "${datasets[@]}"; do
  echo "Processing: $dataset"

  for traversal_pair in "${traversals_combinations[@]}"; do
    IFS=',' read -r sed_first_traversal sed_second_traversal <<< "$traversal_pair"
    echo "  Running SED with traversals: $sed_first_traversal and $sed_second_traversal"

    outdir="$dataset/traversals/$sed_first_traversal---$sed_second_traversal"

    mkdir -p "$outdir"

    cargo run --release -- -d "$dataset/trees_sorted.bracket" --quiet lower-bound --query-file "$dataset/query.csv" --output "$outdir" sed --runs 3 --sed-first-traversal "$sed_first_traversal" --sed-second-traversal "$sed_second_traversal" > "$outdir/query_times.csv"
    cargo run --release -- -d "$dataset/trees_sorted.bracket" --quiet lower-bound --query-file "$dataset/query.csv" --output "$outdir" sed-struct --runs 3 --sed-first-traversal "$sed_first_traversal" --sed-second-traversal "$sed_second_traversal" >> "$outdir/query_times.csv"

    echo "  Done with $traversal_pair"
    echo ""
  done



  echo "  Done with $dataset"

  echo ""
done