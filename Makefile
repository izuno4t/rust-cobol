# COBOL Compiler - Makefile
# Rust workspace 用のビルド/テスト/リント/NIST 操作をまとめる。

.DEFAULT_GOAL := all

CARGO := cargo
BINARY := cobolc
INSTALL_DIR := $(HOME)/.cargo/bin

NIST_ENV_ROOT ?= $(CURDIR)/target/nist
NIST_JOBS ?= 5
NIST_SOURCE_VAL ?=
NIST_COBOLC ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR)/release/cobol-driver,$(CURDIR)/target/release/cobol-driver)
NIST_SCOPE := $(if $(PROGRAM),$(MODULE) $(PROGRAM),$(or $(MODULE),--all))

RUNTIME_X86_IMAGE := rust-cobol-runtime-x86
RUNTIME_X86_TARGET := /workspace/target/runtime-x86-linux-amd64
RUNTIME_X86_NIST_ENV := /workspace/target/nist-x86
RUNTIME_X86_ACTION ?= nist

EMPTY_PROXY_ENV := http_proxy= https_proxy= HTTP_PROXY= HTTPS_PROXY= no_proxy= NO_PROXY=
EMPTY_PROXY_RUN_ARGS := -e http_proxy= -e https_proxy= -e HTTP_PROXY= -e HTTPS_PROXY= -e no_proxy= -e NO_PROXY=

.PHONY: \
	all build release test lint fmt check audit verify clean \
	install uninstall example benchmark \
	nist runtime-x86 \
	nist-prepare nist-run nist-compile nist-audit nist-compare \
	nist-audit-codegen nist-compare-codegen \
	runtime-x86-build runtime-x86-shell runtime-x86-nist runtime-x86-bench \
	help

define require_nist_programs
	@test -d "$(NIST_ENV_ROOT)/programs" || \
		( echo "NIST programs are not prepared in $(NIST_ENV_ROOT)/programs"; \
		  echo "Run 'make nist-prepare' first."; \
		  exit 1 )
endef

## Default
all: release

## Core Development
build:
	mkdir -p target/debug/deps target/debug/build target/debug/examples target/debug/incremental
	$(CARGO) build

release:
	mkdir -p target/release/deps target/release/build target/release/examples target/release/incremental
	$(CARGO) build --release

test:
	$(CARGO) test --workspace

lint:
	mkdir -p target/debug/deps target/debug/build target/debug/examples target/debug/incremental
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) fmt --all -- --check
	cspell "crates/**/*.rs" "docs/**/*.md" "CLAUDE.md"

fmt:
	$(CARGO) fmt --all

check:
	$(CARGO) check --workspace

audit:
	$(MAKE) check
	$(MAKE) lint

verify:
	$(MAKE) clean
	$(MAKE) audit
	$(MAKE) test
	$(MAKE) nist-prepare
	$(MAKE) nist-audit
	$(MAKE) nist-compare
	$(MAKE) nist-compile

clean:
	$(CARGO) clean

install: release
	cp target/release/cobol-driver $(INSTALL_DIR)/$(BINARY)
	@echo "Installed $(BINARY) to $(INSTALL_DIR)/$(BINARY)"

uninstall:
	rm -f $(INSTALL_DIR)/$(BINARY)
	@echo "Removed $(BINARY) from $(INSTALL_DIR)"

example: release
	$(CARGO) run --release --package cobol-driver -- examples/hello.cob -o /tmp/hello --source-format free
	@echo "--- Running hello ---"
	@/tmp/hello

benchmark:
	bash tests/benchmark/nqueens/run.sh
	bash tests/benchmark/micro/run.sh

## NIST CCVS 85
nist: nist-run

nist-prepare:
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" bash tests/nist/bin/prepare.sh $(NIST_SOURCE_VAL)

nist-run:
	@test -x "$(NIST_COBOLC)" || $(MAKE) release
	$(require_nist_programs)
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/bin/run.sh $(NIST_SCOPE)

nist-compile:
	@test -x "$(NIST_COBOLC)" || $(MAKE) release
	$(require_nist_programs)
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/bin/run.sh --compile $(NIST_SCOPE)

nist-audit: nist-audit-codegen

nist-audit-codegen:
	@test -x "$(NIST_COBOLC)" || $(MAKE) release
	$(require_nist_programs)
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/bin/audit-codegen.sh $(NIST_SCOPE)

nist-compare: nist-compare-codegen

nist-compare-codegen:
	@test -d "$(NIST_ENV_ROOT)/audit/codegen" || \
		( echo "Audit results are not prepared in $(NIST_ENV_ROOT)/audit/codegen"; \
		  echo "Run 'make nist-audit' first."; \
		  exit 1 )
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" \
	NIST_JOBS="$(NIST_JOBS)" \
	bash tests/nist/bin/compare-codegen.sh $(NIST_SCOPE)

## x86 Runtime Validation
runtime-x86:
	@case "$(RUNTIME_X86_ACTION)" in \
		build) $(MAKE) runtime-x86-build ;; \
		shell) $(MAKE) runtime-x86-shell ;; \
		nist) $(MAKE) runtime-x86-nist MODULE="$(MODULE)" PROGRAM="$(PROGRAM)" NIST_JOBS="$(NIST_JOBS)" ;; \
		bench) $(MAKE) runtime-x86-bench ;; \
		*) echo "Unknown RUNTIME_X86_ACTION=$(RUNTIME_X86_ACTION). Use build|shell|nist|bench."; exit 2 ;; \
	esac

runtime-x86-build:
	$(EMPTY_PROXY_ENV) docker build -f docker/runtime-x86.Dockerfile -t $(RUNTIME_X86_IMAGE) .

runtime-x86-shell:
	docker run --rm -it \
		-v "$(CURDIR):/workspace" \
		$(EMPTY_PROXY_RUN_ARGS) \
		-e CARGO_TARGET_DIR=$(RUNTIME_X86_TARGET) \
		$(RUNTIME_X86_IMAGE) \
		bash

runtime-x86-nist:
	docker run --rm \
		-v "$(CURDIR):/workspace" \
		$(EMPTY_PROXY_RUN_ARGS) \
		-e CARGO_TARGET_DIR=$(RUNTIME_X86_TARGET) \
		-e NIST_ENV_ROOT=$(RUNTIME_X86_NIST_ENV) \
		$(RUNTIME_X86_IMAGE) \
		bash -lc "make nist-prepare && make nist NIST_JOBS=$(NIST_JOBS) $(if $(PROGRAM),MODULE=$(MODULE) PROGRAM=$(PROGRAM),$(if $(MODULE),MODULE=$(MODULE),))"

runtime-x86-bench:
	docker run --rm \
		-v "$(CURDIR):/workspace" \
		$(EMPTY_PROXY_RUN_ARGS) \
		-e CARGO_TARGET_DIR=$(RUNTIME_X86_TARGET) \
		$(RUNTIME_X86_IMAGE) \
		bash -lc "bash tests/benchmark/nqueens/run.sh && bash tests/benchmark/micro/run.sh --compare gnucobol"

## Help
help:
	@echo "使用可能なターゲット:"
	@echo ""
	@echo "  基本:"
	@echo "    make                 リリースビルド"
	@echo "    make build           デバッグビルド"
	@echo "    make test            ワークスペース全体のテスト"
	@echo "    make lint            clippy + rustfmt check + cspell"
	@echo "    make clean           ビルド成果物を削除"
	@echo "    make example         examples/hello.cob をコンパイルして実行"
	@echo "    make benchmark       nqueens と micro ベンチマークを実行"
	@echo ""
	@echo "  NIST:"
	@echo "    個別実行するもの（MODULE/PROGRAM 指定可）:"
	@echo "      make nist [MODULE=NC]                 コンパイル、実行、判定、集計"
	@echo "      make nist PROGRAM=NC101A              指定プログラムだけを実行"
	@echo "      make nist-compile [MODULE=NC]         実行せずコンパイル可否だけを確認"
	@echo "      make nist-audit [MODULE=NC]           HIR/C 生成物を監査用に出力"
	@echo "      make nist-compare [MODULE=NC]         audit 結果の COBOL/C シンボルを比較"
	@echo "    個別実行しないもの（全体準備）:"
	@echo "      make nist-prepare                     CCVS 入力を target/nist/programs に展開"
	@echo ""
	@echo "  x86 runtime:"
	@echo "    make runtime-x86 RUNTIME_X86_ACTION=build   x86_64 Linux Docker image を作成"
	@echo "    make runtime-x86 RUNTIME_X86_ACTION=shell   x86_64 Linux コンテナで shell を開く"
	@echo "    make runtime-x86 [MODULE=NC]                x86_64 Linux コンテナで NIST を実行"
	@echo "    make runtime-x86 RUNTIME_X86_ACTION=bench   x86_64 Linux コンテナで benchmark を実行"
	@echo ""
	@echo "  make help"
