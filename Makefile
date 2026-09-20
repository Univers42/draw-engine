# **************************************************************************** #
#                                                                              #
#                                                         :::      ::::::::    #
#    Makefile                                           :+:      :+:    :+:    #
#                                                     +:+ +:+         +:+      #
#    By: dlesieur <dlesieur@student.42.fr>          +#+  +:+       +#+         #
#                                                 +#+#+#+#+#+   +#+            #
#    Created: 2026/09/20 00:00:00 by dlesieur          #+#    #+#              #
#    Updated: 2026/09/20 00:00:00 by dlesieur         ###   ########.fr        #
#                                                                              #
# **************************************************************************** #

SHELL := /bin/bash
DC    := docker compose

CYAN  := \033[36m
GREEN := \033[32m
RESET := \033[0m

.DEFAULT_GOAL := help

help: ## Show available targets
	@grep -hE '^[a-zA-Z_-]+:.*## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*## "}; {printf "$(CYAN)%-16s$(RESET) %s\n", $$1, $$2}'

build: ## Build the draw-engine image
	$(DC) build
	@echo -e "$(GREEN)✔ image built$(RESET)"

install: build ## pnpm install inside Docker (frozen lockfile)
	$(DC) run --rm --no-deps draw-engine bash scripts/docker-run.sh install
	@echo -e "$(GREEN)✔ pnpm install$(RESET)"

lock: build ## Regenerate pnpm-lock.yaml inside Docker
	$(DC) run --rm --no-deps --entrypoint sh draw-engine -lc \
		'pnpm install --lockfile-only --no-frozen-lockfile --store-dir /pnpm/store'
	@echo -e "$(GREEN)✔ pnpm-lock.yaml updated$(RESET)"

test: ## Run the node:test suite in Docker via pnpm
	$(DC) run --rm --no-deps draw-engine bash scripts/docker-run.sh test

quality: ## Quality gate (currently the test suite) in Docker via pnpm
	$(DC) run --rm --no-deps draw-engine bash scripts/docker-run.sh quality

shell: ## Interactive shell in the draw-engine container
	$(DC) run --rm --no-deps draw-engine sh

up: build ## Start a long-running container
	$(DC) up -d
	@echo -e "$(GREEN)✔ draw-engine up$(RESET)"

down: ## Stop and remove containers
	$(DC) down
	@echo -e "$(GREEN)✔ down$(RESET)"

clean: ## Remove containers, volumes, and the local image
	$(DC) down -v --rmi local
	@echo -e "$(GREEN)✔ clean$(RESET)"

.PHONY: help build install lock test quality shell up down clean
