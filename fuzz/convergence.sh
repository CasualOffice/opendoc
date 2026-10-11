#!/usr/bin/env bash
# Deterministic multi-replica convergence soak through real relay WebSockets.
# Reproduce the default CI corpus with 24 seeds; pass a larger count to soak.
set -euo pipefail
cd "$(dirname "$0")/.."
OPENDOC_CONVERGENCE_SEEDS="${1:-24}" cargo test -p opendoc-relay --locked randomized_relay_convergence -- --nocapture
