# COBOL Compiler - Makefile
# Rust workspace 用のビルド/テスト/リント操作をまとめたもの

CARGO := cargo
BINARY := cobolc
INSTALL_DIR := $(HOME)/.cargo/bin
NIST_COBOLC ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR)/release/cobol-driver,$(CURDIR)/target/release/cobol-driver)

.PHONY: all build release test test-unit test-e2e lint fmt check clippy clean install uninstall example spellcheck nist-prepare nist-run nist-summary runtime-x86-build runtime-x86-shell runtime-x86-nist runtime-x86-bench help

NIST_ENV_ROOT ?= $(CURDIR)/target/nist
NIST_JOBS ?= 1
NIST_SOURCE_VAL ?=
RUNTIME_X86_IMAGE := rust-cobol-runtime-x86
RUNTIME_X86_TARGET := /workspace/target/runtime-x86-linux-amd64
EMPTY_PROXY_ENV := http_proxy= https_proxy= HTTP_PROXY= HTTPS_PROXY= no_proxy= NO_PROXY=
EMPTY_PROXY_RUN_ARGS := -e http_proxy= -e https_proxy= -e HTTP_PROXY= -e HTTPS_PROXY= -e no_proxy= -e NO_PROXY=

## デフォルト: リリースビルド
all: release

## デバッグビルド
build:
	$(CARGO) build

## リリースビルド
release:
	$(CARGO) build --release

## 全テスト実行
test:
	$(CARGO) test --workspace

## ユニットテストのみ
test-unit:
	$(CARGO) test --workspace --lib

## E2Eテストのみ
test-e2e:
	$(CARGO) test --package cobol-driver --test e2e_test

## リント (clippy + fmt check + spellcheck)
lint: clippy fmt-check spellcheck

## clippy
clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## フォーマットチェック
fmt-check:
	$(CARGO) fmt --all -- --check

## フォーマット適用
fmt:
	$(CARGO) fmt --all

## スペルチェック
spellcheck:
	cspell "crates/**/*.rs" "docs/**/*.md" "CLAUDE.md"

## 型チェック (コンパイルなし)
check:
	$(CARGO) check --workspace

## ビルド成果物のクリーン
clean:
	$(CARGO) clean

## バイナリのインストール ($HOME/.cargo/bin/cobolc)
install: release
	cp target/release/cobol-driver $(INSTALL_DIR)/$(BINARY)
	@echo "Installed $(BINARY) to $(INSTALL_DIR)/$(BINARY)"

## バイナリのアンインストール
uninstall:
	rm -f $(INSTALL_DIR)/$(BINARY)
	@echo "Removed $(BINARY) from $(INSTALL_DIR)"

## examples/hello.cob のコンパイルと実行
example: release
	$(CARGO) run --release --package cobol-driver -- examples/hello.cob -o /tmp/hello --source-format free
	@echo "--- Running hello ---"
	@/tmp/hello

## NIST CCVS 85 テスト準備
nist-prepare:
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" bash tests/nist/prepare.sh $(NIST_SOURCE_VAL)

## NIST CCVS 85 テスト実行
nist-run:
	@test -x "$(NIST_COBOLC)" || $(MAKE) release
	@test -d "$(NIST_ENV_ROOT)/programs" || \
		( echo "NIST programs are not prepared in $(NIST_ENV_ROOT)/programs"; \
		  echo "Run 'make nist-prepare' first."; \
		  exit 1 )
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/run_nist.sh $(if $(PROGRAM),$(MODULE) $(PROGRAM),$(or $(MODULE),--all))

## NIST 結果サマリー
nist-summary:
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" bash tests/nist/run_nist.sh --summary

## x86 ランタイム検証環境のビルド
runtime-x86-build:
	$(EMPTY_PROXY_ENV) docker build -f docker/runtime-x86.Dockerfile -t $(RUNTIME_X86_IMAGE) .

## x86 ランタイム検証環境のシェル
runtime-x86-shell:
	docker run --rm -it \
		-v "$(CURDIR):/workspace" \
		$(EMPTY_PROXY_RUN_ARGS) \
		-e CARGO_TARGET_DIR=$(RUNTIME_X86_TARGET) \
		$(RUNTIME_X86_IMAGE) \
		bash

## x86 ランタイム検証環境で NIST 実行
runtime-x86-nist:
	docker run --rm \
		-v "$(CURDIR):/workspace" \
		$(EMPTY_PROXY_RUN_ARGS) \
		-e CARGO_TARGET_DIR=$(RUNTIME_X86_TARGET) \
		$(RUNTIME_X86_IMAGE) \
		bash -lc "make nist-prepare && make nist-run $(if $(PROGRAM),MODULE=$(MODULE) PROGRAM=$(PROGRAM),$(if $(MODULE),MODULE=$(MODULE),))"

## x86 ランタイム検証環境でベンチマーク実行
runtime-x86-bench:
	docker run --rm \
		-v "$(CURDIR):/workspace" \
		$(EMPTY_PROXY_RUN_ARGS) \
		-e CARGO_TARGET_DIR=$(RUNTIME_X86_TARGET) \
		$(RUNTIME_X86_IMAGE) \
		bash -lc "bash benchmarks/run_benchmark.sh && bash tests/benchmark/run_bench.sh --compare gnucobol"

## ヘルプ
help:
	@echo "使用可能なターゲット:"
	@echo "  make build       - デバッグビルド"
	@echo "  make release     - リリースビルド (デフォルト)"
	@echo "  make test        - 全テスト実行"
	@echo "  make test-unit   - ユニットテストのみ"
	@echo "  make test-e2e    - E2Eテストのみ"
	@echo "  make lint        - clippy + フォーマットチェック + スペルチェック"
	@echo "  make clippy      - clippy のみ"
	@echo "  make fmt-check   - フォーマットチェックのみ"
	@echo "  make spellcheck  - スペルチェック"
	@echo "  make fmt         - フォーマット適用"
	@echo "  make check       - 型チェック (コンパイルなし)"
	@echo "  make clean       - ビルド成果物のクリーン"
	@echo "  make install     - cobolc を ~/.cargo/bin にインストール"
	@echo "  make uninstall   - cobolc をアンインストール"
	@echo "  make example     - examples/hello.cob をコンパイル・実行"
	@echo "  make nist-prepare - NIST 資材を target/nist/programs に展開"
	@echo "  make nist-run     - NIST CCVS 85 全モジュール実行"
	@echo "  make nist-run NIST_JOBS=4 - NIST 全モジュールを4並列で実行"
	@echo "  make nist-run MODULE=NC - NIST 特定モジュール実行"
	@echo "  make nist-run MODULE=NC PROGRAM=NC101A - NIST 単一プログラム実行"
	@echo "  make nist-summary - NIST 結果サマリー表示"
	@echo "  make runtime-x86-build - x86 ランタイム検証環境をビルド"
	@echo "  make runtime-x86-shell - x86 ランタイム検証環境に入る"
	@echo "  make runtime-x86-nist MODULE=NC - x86 環境で NIST を実行"
	@echo "  make runtime-x86-bench - x86 環境でベンチマークを実行"
	@echo "  make help        - このヘルプ表示"
