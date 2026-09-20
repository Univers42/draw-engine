# syntax=docker/dockerfile:1.7
# Dockerfile
#
# Alpine + Node 22 + pnpm. Matches the osionos playground image.
# Bind-mount the repo at /app for tests; named volumes hold node_modules
# and the pnpm store. Entrypoint installs when the lockfile stamp is stale.

FROM public.ecr.aws/docker/library/node:22-alpine

RUN --mount=type=cache,target=/var/cache/apk,sharing=locked \
	apk add --update-cache git bash

ENV PNPM_HOME=/pnpm
ENV PATH=$PNPM_HOME:$PATH
ENV DRAW_ENGINE_IN_DOCKER=1

RUN corepack enable \
	&& corepack prepare pnpm@10.32.1 --activate \
	&& mkdir -p /pnpm/store \
	&& pnpm config set store-dir /pnpm/store

WORKDIR /app

COPY scripts/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["pnpm", "test"]
