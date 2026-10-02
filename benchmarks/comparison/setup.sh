#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
python3 -m venv .benchmark-bootstrap
.benchmark-bootstrap/bin/pip install 'uv==0.12.15'
.benchmark-bootstrap/bin/uv venv --python 3.12 .benchmark-venv
.benchmark-bootstrap/bin/uv pip sync --python .benchmark-venv/bin/python benchmarks/comparison/requirements.lock
npm install --prefix .benchmark-tools '@probelabs/probe@0.6.0-rc339'
# npm may disable dependency install scripts; this installs Probe's shipped binary.
node .benchmark-tools/node_modules/@probelabs/probe/scripts/postinstall.js
cargo build --release --bin flexcontext
.benchmark-venv/bin/python -m unittest discover -s benchmarks/comparison -p 'test_*.py'
