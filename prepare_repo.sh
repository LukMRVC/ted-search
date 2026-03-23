#!/bin/bash

# Check for required tools
for tool in cargo rustc cmake; do
  if ! command -v "$tool" &> /dev/null; then
    echo "Error: $tool is not installed"
    exit 1
  fi
done


git submodule update --init --recursive

cargo build && cargo test && cargo build --release

mkdir -p tree-similarity/build && cd tree-similarity/build
cmake .. -DCMAKE_BUILD_TYPE=Release
make -j$(nproc)
cd ../../

