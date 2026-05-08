#!/usr/bin/env make

# Legacy Audio Provisioner - Build & Test Makefile

.PHONY: help build release test test-verbose test-unit test-integration clean run run-list dev docs

CARGO := cargo
PROVISION_BIN := lap-bin-provision

help:
	@echo "Legacy Audio Provisioner - Build Tasks"
	@echo ""
	@echo "Available commands:"
	@echo "  make build          - Build debug binary"
	@echo "  make release        - Build optimized release binary"
	@echo "  make test           - Run all tests (quiet)"
	@echo "  make test-verbose   - Run all tests (verbose)"
	@echo "  make test-unit      - Run unit tests only"
	@echo "  make coverage       - Generate test coverage"
	@echo "  make clean          - Remove build artifacts"
	@echo "  make run            - Run with example args (requires USB mount)"
	@echo "  make dev            - Build + watch for changes (requires cargo-watch)"
	@echo "  make docs           - Generate HTML documentation"
	@echo "  make fmt            - Format code (cargo fmt)"
	@echo "  make lint           - Run clippy linter"
	@echo "  make all            - Build release + test + lint"
	@echo ""

build:
	@echo "🔨 Building workspace (debug)..."
	@$(CARGO) build --workspace
	@echo "✓ Build completed for workspace crates"

release:
	@echo "🔨 Building workspace (release)..."
	@$(CARGO) build --workspace --release
	@echo "✓ Release build completed for workspace crates"

test:
	@echo "🧪 Running all tests..."
	@$(CARGO) test --quiet

test-verbose:
	@echo "🧪 Running tests (verbose)..."
	@$(CARGO) test -- --nocapture --test-threads=1

test-unit:
	@echo "🧪 Running unit tests..."
	@$(CARGO) test -p lap-core --lib

coverage:
	@echo "📊 Generating coverage report..."
	@echo "Note: requires cargo-tarpaulin"
	@echo "Install with: cargo install cargo-tarpaulin"
	@$(CARGO) tarpaulin --out Html --output-dir ./coverage

clean:
	@echo "🧹 Cleaning build artifacts..."
	@$(CARGO) clean
	@rm -rf coverage/
	@echo "✓ Cleaned"

run: build
	@echo "🚀 Running with example arguments..."
	@echo "Note: modify the paths below to match your system"
	@$(CARGO) run -p $(PROVISION_BIN) -- \
		provision \
		--usb /tmp/usb_test \
		--source . \
		--dry-run

run-list: build
	@echo "📋 Listing USB devices..."
	@$(CARGO) run -p $(PROVISION_BIN) -- list

dev:
	@echo "👀 Watching for changes..."
	@echo "Note: requires cargo-watch"
	@echo "Install with: cargo install cargo-watch"
	@cargo watch -x build -x test

docs:
	@echo "📖 Generating documentation..."
	@$(CARGO) doc --no-deps --open

fmt:
	@echo "📐 Formatting code..."
	@$(CARGO) fmt

lint:
	@echo "🔍 Running clippy linter..."
	@$(CARGO) clippy -- -D warnings

all: lint test release
	@echo ""
	@echo "✓ All checks passed!"
	@echo "✓ Release binary ready: ./target/release/$(BINARY)"

# Default target
.DEFAULT_GOAL := help
