#!/bin/bash

# Load datasets from datasets directory into variable
datasets_dir="./datasets"
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
  # Add your experiment logic here
  mkdir -p "$dataset/statistics"
  cargo run --release -- -d "$dataset/trees_sorted.bracket" --quiet statistics --hists "$dataset/statistics" | tail -n 2 >  "$dataset/statistics/collection.csv"
done


datasets_dir="./ukkonen-vs-regular-sed-struct-test"
datasets=()

# Iterate over dataset files
for dataset in "$datasets_dir"/*; do
  if [ -d "$dataset" ]; then
    datasets+=("$dataset")
  fi
done

for dataset in "${datasets[@]}"; do
  echo "Processing: $dataset"
  # Add your experiment logic here
  mkdir -p "$dataset/statistics"
  cargo run --release -- -d "$dataset/trees_sorted.bracket" --quiet statistics --hists "$dataset/statistics" | tail -n 2 >  "$dataset/statistics/collection.csv"
done
