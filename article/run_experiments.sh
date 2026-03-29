#!/bin/bash


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

exit 0;

# Load datasets from datasets directory into variable
datasets_dir="./datasets"
datasets=()
SKIP_TARGET="ukkonen-vs-regular-sed-struct-test"

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
  
  # Run SED and capture output
  echo "  Running LB filters..."
  cargo run --release -- -d "$dataset/trees_sorted.bracket" --queries "$dataset/query.csv" --output "$dataset" --runs 3 > "$dataset/query_times.csv"
  
  echo "Done with $dataset"
done

echo "All datasets processed!"


datasets_dir="./datasets"
datasets=("$datasets_dir/dblp" "$datasets_dir/sentiment")

selectivities=(2 3 5 10)


for dataset in "${datasets[@]}"; do
  echo "Processing: $dataset"

  for sel in "${selectivities[@]}"; do
    echo "  Selectivity: $sel"

    candidates=()
    while IFS= read -r file; do
      candidates+=("$dataset/selectivity-$sel/$file")
    done < <(ls "$dataset/selectivity-$sel/" | grep _candidates.csv)

    echo "    Validating candidates... ${#candidates[@]} files found."
    ../query_validate_3 "$dataset/trees_sorted.bracket" "$dataset/selectivity-$sel/query-$sel-100.csv" "${candidates[@]}" > "$dataset/selectivity-$sel/verified-all.csv"
  done
done

