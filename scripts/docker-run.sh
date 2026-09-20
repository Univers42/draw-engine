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

run_tests() {
  shopt -s globstar nullglob
  local files=(src/**/*.test.ts tests/**/*.test.ts)
  exec node --experimental-strip-types --import ./scripts/ts-register.mjs --test-reporter=spec --test "${files[@]}" "$@"
}

run_inside_container() {
  case "${COMMAND}" in
    test|quality) run_tests "$@" ;;
    install) exec pnpm install --frozen-lockfile --prefer-offline --store-dir /pnpm/store "$@" ;;
    lock) exec pnpm install --lockfile-only --no-frozen-lockfile --store-dir /pnpm/store "$@" ;;
    *) echo "Unknown docker-run command: ${COMMAND}" >&2; exit 2 ;;
  esac
}

if [[ -f /.dockerenv || "${DRAW_ENGINE_IN_DOCKER:-}" == "1" ]]; then
  run_inside_container "$@"
fi

if ! docker info >/dev/null 2>&1; then
  echo "[docker-run] Docker is required for '${COMMAND}'." >&2
  exit 1
fi

compose=(docker compose)

case "${COMMAND}" in
  test|quality|install|lock)
    "${compose[@]}" run --rm --no-deps draw-engine bash scripts/docker-run.sh "${COMMAND}" "$@"
    ;;
  *)
    echo "Usage: $0 {test|quality|install|lock}" >&2
    exit 2
    ;;
esac
