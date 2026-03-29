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


mkdir -p external-sources/build && cd external-sources/build && cmake .. -DCMAKE_BUILD_TYPE=Release
make -j$(nproc)
cd ../../

wget -P article --show-progress "https://github.com/LukMRVC/ted-search/releases/download/datasets-v1.0/datasets.tar.zst"
wget -P article "https://github.com/LukMRVC/ted-search/releases/download/datasets-v1.0/datasets.tar.zst.sha256"

cd article;

if sha256sum --check --status datasets.tar.zst.sha256; then
    echo "Checksum passed!"
else
    echo "Checksum failed! Exiting."
    exit 1
fi

tar --zstd -xf datasets.tar.zst

cd ..

