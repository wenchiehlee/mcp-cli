# Makefile for mcp-cli (Pure Rust CLI)
# This file provides common tasks for compilation, verification, testing, and release management.

# Compiler and tool definitions
CARGO = cargo

# Default action when running 'make'
.DEFAULT_GOAL := help

.PHONY: all
all: build-release ## Build the application in release mode (fully optimized)

.PHONY: build
build: ## Build the application in debug mode
	@echo "==> Building in debug mode..."
	$(CARGO) build

.PHONY: build-release
build-release: ## Build the application in release mode (fully optimized)
	@echo "==> Building in release mode..."
	$(CARGO) build --release

.PHONY: install
install: build-release ## Install the compiled release binary to ~/.local/bin
	@echo "==> Installing binary to ~/.local/bin..."
	@mkdir -p $(HOME)/.local/bin
	@cp target/release/mcp-cli $(HOME)/.local/bin/mcp-cli
	@echo "==> Installation complete! Make sure ~/.local/bin is in your PATH."

.PHONY: run
run: ## Run the application in debug mode
	$(CARGO) run --

.PHONY: test
test: ## Run the native Rust unit tests
	@echo "==> Running unit tests..."
	$(CARGO) test --all-features

.PHONY: fmt
fmt: ## Check and enforce Rust formatting standards
	@echo "==> Checking code formatting..."
	$(CARGO) fmt --all -- --check

.PHONY: fmt-fix
fmt-fix: ## Format all Rust source files automatically
	@echo "==> Formatting code..."
	$(CARGO) fmt --all

.PHONY: clippy
clippy: ## Run clippy for static analysis and lint checks
	@echo "==> Running clippy lints..."
	$(CARGO) clippy --all-targets --all-features -- -D warnings

.PHONY: clean
clean: ## Remove compiled target files
	@echo "==> Cleaning build artifacts..."
	$(CARGO) clean

.PHONY: release
release: ## Trigger a new release (usage: make release VERSION=X.Y.Z)
	@if [ -z "$(VERSION)" ]; then \
		echo "Error: VERSION is required. Usage: make release VERSION=0.3.1"; \
		exit 1; \
	fi
	@echo "==> Initiating release v$(VERSION)..."
	./scripts/release.sh $(VERSION)

.PHONY: help
help: ## Show this help menu with descriptions of each command
	@echo "mcp-cli Management Tasks:"
	@echo "--------------------------------------------------------"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@echo "--------------------------------------------------------"
