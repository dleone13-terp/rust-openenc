#!/bin/bash
echo "Generating sprites..."
cargo run -- --sprites-output sprites/themed

echo "Generating styles..."
mkdir -p styles
for t in day dusk night; do
  cargo run -- --style-output styles/$t.json --theme $t
done
