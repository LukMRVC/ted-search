#!/bin/bash

# Load datasets from datasets directory into variable
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

    cargo run --release -- --formatted --runs 3 --dataset "$dataset/trees_sorted.bracket" --queries "$dataset/query.csv" --output "$outdir" sed --sed-traversal-first "$sed_first_traversal" --sed-traversal-second "$sed_second_traversal" > "$outdir/query_times.csv"
    cargo run --release -- --formatted --runs 3 --dataset "$dataset/trees_sorted.bracket" --queries "$dataset/query.csv" --output "$outdir" sed-struct --sed-traversal-first "$sed_first_traversal" --sed-traversal-second "$sed_second_traversal" >> "$outdir/query_times.csv"

    echo "  Done with $traversal_pair"
    echo ""
  done


  echo "  Done with $dataset"
  echo ""
done

# Load datasets from datasets directory into variable
datasets_dir="./datasets"
datasets=()
SKIP_TARGET="ukkonen-vs-regular-sed-struct-test"
methods=("sed" "sed-struct" "structural" "bib" "lblint")



# Iterate over dataset files
for dataset in "$datasets_dir"/*; do
  if [ "$dataset" == "$SKIP_TARGET" ]; then
    echo "Skipping dataset: $dataset (Target match)"
    continue;
  fi

  if [ -d "$dataset" ]; then
    datasets+=("$dataset")
  fi
done

# Process each dataset
for dataset in "${datasets[@]}"; do
  echo "Processing: $dataset"

  test -f "$dataset/query_times.csv" && rm "$dataset/query_times.csv"
  
  # Run LB 
  echo "  Running LB filters..."
  for method in "${methods[@]}"; do
    echo "    Method: $method"
    cargo run --release --  --formatted --runs 3 --dataset "$dataset/trees_sorted.bracket" --queries "$dataset/query.csv" --output "$dataset" "$method" --sed-traversal-first reversed-preorder --sed-traversal-second preorder >> "$dataset/query_times.csv"
  done

  ../external-sources/build/query_validate "$dataset/trees_sorted.bracket" "$dataset/query.csv" "$dataset/Sed_candidates.csv" \
  "$dataset/SEDStruct_candidates.csv" "$dataset/Bib_candidates.csv" "$dataset/Structural_candidates.csv" "$dataset/Lblint_candidates.csv" > "$dataset/verified-all.csv"
  
  echo "Done with $dataset"
done

echo "All datasets processed!"


# datasets_dir="./datasets"
# datasets=("$datasets_dir/dblp" "$datasets_dir/sentiment")

# selectivities=(2 3 5 10)


# for dataset in "${datasets[@]}"; do
#   echo "Processing: $dataset"

#   for sel in "${selectivities[@]}"; do
#     echo "  Selectivity: $sel"

#     candidates=()
#     while IFS= read -r file; do
#       candidates+=("$dataset/selectivity-$sel/$file")
#     done < <(ls "$dataset/selectivity-$sel/" | grep _candidates.csv)

#     echo "    Validating candidates... ${#candidates[@]} files found."
#     ../external-sources/build/query_validate "$dataset/trees_sorted.bracket" "$dataset/selectivity-$sel/query-$sel-100.csv" "${candidates[@]}" > "$dataset/selectivity-$sel/verified-all.csv"
#   done
# done


# Load Ukkonen vs regular SED dataset
datasets_dir="./datasets/ukkonen-vs-regular-sed-struct-test"
datasets=()

# Iterate over dataset files
for dataset in "$datasets_dir"/*; do
  if [ -d "$dataset" ]; then
    datasets+=("$dataset")
  fi
done

# Process each dataset
for dataset in "${datasets[@]}"; do
  echo "Processing: $dataset"
  
  # Run SED and capture output
  echo "  Running Exact SED filter..."
  cargo run --release -- --formatted --runs 3 --dataset "$dataset/trees_sorted.bracket" --queries "$dataset/query.csv" --output "$dataset" sed-exact >> "$dataset/query-sed-regular.csv"
  echo "  Running Ukkonen SED filter..."
  
  cargo run --release -- --formatted --runs 3 --dataset "$dataset/trees_sorted.bracket" --queries "$dataset/query.csv" --output "$dataset" sed >> "$dataset/query-sed-ukkonen.csv"
  
  echo "Done with $dataset"
done