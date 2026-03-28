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

# Function to update a specific method's results in query_times.csv
update_method_results() {
  local dataset="$1"
  local method_name="$2"  # e.g., "Sed" or "SEDStruct"
  local temp_output="$3"
  
  local query_file="$dataset/query_times.csv"
  
  if [ ! -f "$query_file" ]; then
    echo "  Warning: $query_file not found, skipping update"
    return
  fi
  
  # Extract the method section from temp output (3 lines: method name, time, candidates)
  local method_section=$(grep -A 2 "^${method_name}$" "$temp_output")
  
  if [ -n "$method_section" ]; then
    # Use awk to replace only the specific method section
    local temp_file=$(mktemp)
    awk -v method="$method_name" '
      BEGIN { in_section = 0; skip_count = 0; replaced = 0 }
      {
        if (skip_count > 0) {
          skip_count--
          next
        }
        if ($0 == method && !replaced) {
          system("grep -A 2 \"^" method "$\" " ARGV[2])
          in_section = 1
          skip_count = 2
          replaced = 1
          next
        }
        print $0
      }
    ' "$query_file" "$temp_output" > "$temp_file"
    
    mv "$temp_file" "$query_file"
    echo "  Updated $method_name results in query_times.csv"
  else
    echo "  Warning: Could not find $method_name results in output"
  fi
}

# Process each dataset
for dataset in "${datasets[@]}"; do
  echo "Processing: $dataset"
  
  # Run SED and capture output
  echo "  Running SED method..."
  temp_sed=$(mktemp)
  cargo run --release -- -d "$dataset/trees_sorted.bracket" --quiet lower-bound --query-file "$dataset/query.csv" --output "$dataset" sed --runs 3 > "$temp_sed"
  update_method_results "$dataset" "Sed" "$temp_sed"
  rm "$temp_sed"
  
  # Run SED-Struct and capture output
  echo "  Running SEDStruct method..."
  temp_sedstruct=$(mktemp)
  cargo run --release -- -d "$dataset/trees_sorted.bracket" --quiet lower-bound --query-file "$dataset/query.csv" --output "$dataset" sed-struct --runs 3 > "$temp_sedstruct"
  update_method_results "$dataset" "SEDStruct" "$temp_sedstruct"
  rm "$temp_sedstruct"
  
  echo "  Done with $dataset"
  echo ""
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
