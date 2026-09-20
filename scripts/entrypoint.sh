#!/bin/sh
# **************************************************************************** #
#                                                                              #
#                                                         :::      ::::::::    #
#    entrypoint.sh                                      :+:      :+:    :+:    #
#                                                     +:+ +:+         +:+      #
#    By: dlesieur <dlesieur@student.42.fr>          +#+  +:+       +#+         #
#                                                 +#+#+#+#+#+   +#+            #
#    Created: 2026/09/20 00:00:00 by dlesieur          #+#    #+#              #
#    Updated: 2026/09/20 00:00:00 by dlesieur         ###   ########.fr        #
#                                                                              #
# **************************************************************************** #
set -e

export DRAW_ENGINE_IN_DOCKER=1
git config --global --add safe.directory /app 2>/dev/null || true

STAMP_DIR="/app/node_modules/.cache"
STAMP="${STAMP_DIR}/pnpm-deps.sha256"

if [ ! -f /app/pnpm-lock.yaml ]; then
  echo "[entrypoint] No lockfile — generating with pnpm…"
  pnpm install --lockfile-only --no-frozen-lockfile --store-dir /pnpm/store
fi

current_hash="$(cat /app/package.json /app/pnpm-lock.yaml | sha256sum | awk '{print $1}')"
cached_hash=""
if [ -f "$STAMP" ]; then
  cached_hash="$(cat "$STAMP")"
fi

if [ ! -d /app/node_modules/.pnpm ] || [ "$cached_hash" != "$current_hash" ]; then
  echo "[entrypoint] Installing dependencies with pnpm…"
  pnpm install --frozen-lockfile --prefer-offline --store-dir /pnpm/store
  mkdir -p "$STAMP_DIR"
  printf '%s' "$current_hash" > "$STAMP"
  echo "[entrypoint] Dependencies ready."
else
  echo "[entrypoint] Dependencies up to date."
fi

exec "$@"
