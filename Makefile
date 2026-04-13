# COBOL Compiler - Makefile
# Rust workspace 用のビルド/テスト/リント操作をまとめたもの

CARGO := cargo
BINARY := cobolc
INSTALL_DIR := $(HOME)/.cargo/bin
NIST_COBOLC ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR)/release/cobol-driver,$(CURDIR)/target/release/cobol-driver)

.PHONY: all build release test lint fmt check audit verify clean install uninstall example nist-prepare nist-compile nist-compile-errors nist-run nist-summary nist-audit-codegen nist-compare-codegen runtime-x86-build runtime-x86-shell runtime-x86-nist runtime-x86-bench help

NIST_ENV_ROOT ?= $(CURDIR)/.nist
NIST_JOBS ?= 5
NIST_SOURCE_VAL ?=
RUNTIME_X86_IMAGE := rust-cobol-runtime-x86
RUNTIME_X86_TARGET := /workspace/target/runtime-x86-linux-amd64
RUNTIME_X86_NIST_ENV := /workspace/target/nist-x86
EMPTY_PROXY_ENV := http_proxy= https_proxy= HTTP_PROXY= HTTPS_PROXY= no_proxy= NO_PROXY=
EMPTY_PROXY_RUN_ARGS := -e http_proxy= -e https_proxy= -e HTTP_PROXY= -e HTTPS_PROXY= -e no_proxy= -e NO_PROXY=

## デフォルト: リリースビルド
all: release

## デバッグビルド
build:
	mkdir -p target/debug/deps target/debug/build target/debug/examples target/debug/incremental
	$(CARGO) build

## リリースビルド
release:
	mkdir -p target/release/deps target/release/build target/release/examples target/release/incremental
	$(CARGO) build --release

## ワークスペース全体のテスト実行
test:
	$(CARGO) test --workspace

## コード品質チェック (clippy + fmt check + spellcheck)
lint:
	mkdir -p target/debug/deps target/debug/build target/debug/examples target/debug/incremental
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) fmt --all -- --check
	cspell "crates/**/*.rs" "docs/**/*.md" "CLAUDE.md"

## フォーマット適用
fmt:
	$(CARGO) fmt --all

## 型チェック (コンパイルなし)
check:
	$(CARGO) check --workspace

## 通常開発向けの監査 (型チェック + lint)
audit:
	$(MAKE) check
	$(MAKE) lint

## コード変更後の標準検証
verify:
	$(MAKE) clean
	$(MAKE) audit
	$(MAKE) test
	$(MAKE) nist-prepare
	$(MAKE) nist-audit-codegen
	$(MAKE) nist-compare-codegen
	$(MAKE) nist-compile

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

## NIST CCVS 85 compile phase のみ実行
nist-compile:
	@test -x "$(NIST_COBOLC)" || $(MAKE) release
	@test -d "$(NIST_ENV_ROOT)/programs" || \
		( echo "NIST programs are not prepared in $(NIST_ENV_ROOT)/programs"; \
		  echo "Run 'make nist-prepare' first."; \
		  exit 1 )
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/run_nist.sh --compile $(if $(PROGRAM),$(MODULE) $(PROGRAM),$(or $(MODULE),--all))

## NIST 結果サマリー
nist-summary:
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" bash tests/nist/run_nist.sh --summary

## NIST compile error の性質別分類
nist-compile-errors:
	@python3 tests/nist/classify_compile_errors.py "$(NIST_ENV_ROOT)"

## NIST 全件の HIR/C 生成監査
nist-audit-codegen:
	@test -x "$(NIST_COBOLC)" || $(MAKE) release
	@test -d "$(NIST_ENV_ROOT)/programs" || \
		( echo "NIST programs are not prepared in $(NIST_ENV_ROOT)/programs"; \
		  echo "Run 'make nist-prepare' first."; \
		  exit 1 )
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/audit_codegen.sh $(if $(PROGRAM),$(MODULE) $(PROGRAM),$(or $(MODULE),--all))

## NIST 全件の COBOL/C 構造比較
nist-compare-codegen:
	@test -d "$(NIST_ENV_ROOT)/audit/codegen" || \
		( echo "Audit results are not prepared in $(NIST_ENV_ROOT)/audit/codegen"; \
		  echo "Run 'make nist-audit-codegen' first."; \
		  exit 1 )
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	bash tests/nist/compare_codegen.sh $(if $(PROGRAM),$(MODULE) $(PROGRAM),$(or $(MODULE),--all))

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
		-e NIST_ENV_ROOT=$(RUNTIME_X86_NIST_ENV) \
		$(RUNTIME_X86_IMAGE) \
		bash -lc "make nist-prepare && make nist-run NIST_JOBS=$(NIST_JOBS) $(if $(PROGRAM),MODULE=$(MODULE) PROGRAM=$(PROGRAM),$(if $(MODULE),MODULE=$(MODULE),))"

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
	@echo "  make test        - ワークスペース全体のテスト実行"
	@echo "  make lint        - clippy + フォーマットチェック + スペルチェック"
	@echo "  make fmt         - フォーマット適用"
	@echo "  make check       - 型チェック (コンパイルなし)"
	@echo "  make audit       - check + lint による通常開発向け監査"
	@echo "  make verify      - clean + audit + test を順に実行"
	@echo "  make clean       - ビルド成果物のクリーン"
	@echo "  make install     - cobolc を ~/.cargo/bin にインストール"
	@echo "  make uninstall   - cobolc をアンインストール"
	@echo "  make example     - examples/hello.cob をコンパイル・実行"
	@echo "  make nist-prepare - NIST 資材を .nist/programs に展開"
	@echo "  make nist-compile - NIST compile phase のみ実行"
	@echo "  make nist-compile-errors - NIST compile error を性質別に分類"
	@echo "  make nist-run     - NIST CCVS 85 全モジュール実行"
	@echo "  make nist-run NIST_JOBS=4 - NIST 全フェーズを4並列で実行"
	@echo "  make nist-audit-codegen - NIST 全件の HIR/C 生成物を監査出力"
	@echo "  make nist-compare-codegen - NIST 全件の COBOL/C 構造差分を比較"
	@echo "  make nist-run MODULE=NC - NIST 特定モジュール実行"
	@echo "  make nist-run MODULE=NC PROGRAM=NC101A - NIST 単一プログラム実行"
	@echo "  make nist-summary - NIST 結果サマリー表示"
	@echo "  make runtime-x86-build - x86 ランタイム検証環境をビルド"
	@echo "  make runtime-x86-shell - x86 ランタイム検証環境に入る"
	@echo "  make runtime-x86-nist MODULE=NC - x86 環境で NIST を実行"
	@echo "  make runtime-x86-bench - x86 環境でベンチマークを実行"
	@echo "  make help        - このヘルプ表示"
