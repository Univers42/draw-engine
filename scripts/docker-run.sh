#!/usr/bin/env bash
# **************************************************************************** #
#                                                                              #
#                                                         :::      ::::::::    #
#    docker-run.sh                                      :+:      :+:    :+:    #
#                                                     +:+ +:+         +:+      #
#    By: dlesieur <dlesieur@student.42.fr>          +#+  +:+       +#+         #
#                                                 +#+#+#+#+#+   +#+            #
#    Created: 2026/09/20 00:00:00 by dlesieur          #+#    #+#              #
#    Updated: 2026/09/20 00:00:00 by dlesieur         ###   ########.fr        #
#                                                                              #
# **************************************************************************** #

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
COMMAND="${1:-test}"
shift || true

cd "${ROOT_DIR}"

run_host_tests() {
  shopt -s globstar nullglob
  local files=(src/host/**/*.test.ts)
  if ((${#files[@]} == 0)); then
    return 0
  fi
  node --experimental-strip-types --import ./scripts/ts-register.mjs --test-reporter=spec --test "${files[@]}" "$@"
}

run_cargo_test() {
  cargo test --workspace --offline "$@" 2>/dev/null || cargo test --workspace "$@"
}

run_quality() {
  cargo fmt --all --check
  cargo clippy --all-targets --all-features -- -D warnings
  # The browser target as well. `src/wasm/` is behind `#[cfg(target_arch = "wasm32")]`,
  # so a host-only clippy pass never compiles a line of it: the painter, the JS bindings
  # and the input plumbing all escaped the gate entirely, and dead code there surfaced
  # only as a warning during `make wasm`, which nothing was checking.
  cargo clippy --target wasm32-unknown-unknown -p draw-engine --all-features -- -D warnings
  run_cargo_test "$@"
  run_host_tests "$@"
}

run_wasm() {
  local target_dir="${CARGO_TARGET_DIR:-target}"
  # Warnings are errors here too, so a warning cannot reach a shipped artifact even when
  # someone builds the bundle without running `quality` first.
  RUSTFLAGS="${RUSTFLAGS:-} -D warnings" \
    cargo build --release --target wasm32-unknown-unknown -p draw-engine
  mkdir -p pkg
  wasm-bindgen --target web --out-dir pkg \
    "${target_dir}/wasm32-unknown-unknown/release/draw_engine.wasm"
}

run_inside_container() {
  case "${COMMAND}" in
    test)
      run_cargo_test "$@"
      run_host_tests "$@"
      ;;
    quality) run_quality "$@" ;;
    wasm) run_wasm "$@" ;;
    install) exec pnpm install --frozen-lockfile --prefer-offline --store-dir /pnpm/store "$@" ;;
    lock) exec pnpm install --lockfile-only --no-frozen-lockfile --store-dir /pnpm/store "$@" ;;
    *) echo "Unknown docker-run command: ${COMMAND}" >&2; exit 2 ;;
  esac
}

if [[ -f /.dockerenv || "${DRAW_ENGINE_IN_DOCKER:-}" == "1" ]]; then
  run_inside_container "$@"
  exit 0
fi

if ! docker info >/dev/null 2>&1; then
  echo "[docker-run] Docker is required for '${COMMAND}'." >&2
  exit 1
fi

compose=(docker compose)

case "${COMMAND}" in
  test|quality|install|lock|wasm)
    "${compose[@]}" run --rm --build --no-deps draw-engine bash scripts/docker-run.sh "${COMMAND}" "$@"
    ;;
  *)
    echo "Usage: $0 {test|quality|install|lock|wasm}" >&2
    exit 2
    ;;
esac
