#!/bin/bash
# Update version in all Cargo.toml files

VERSION=$1

if [ -z "$VERSION" ]; then
  echo "Usage: $0 <version>"
  exit 1
fi

for cargo in crates/*/Cargo.toml; do
  sed -i "s/^version = \".*\"/version = \"$VERSION\"/" "$cargo"
done

echo "Updated version to $VERSION in all Cargo.toml files"