# COBOL Compiler - Makefile
# Rust workspace 用のビルド/テスト/リント操作をまとめたもの

CARGO := cargo
BINARY := cobolc
INSTALL_DIR := $(HOME)/.cargo/bin
NIST_COBOLC := $(CURDIR)/target/release/cobol-driver

.PHONY: all build release test test-unit test-e2e lint fmt check clippy clean install uninstall example spellcheck nist-prepare nist-run nist-summary help

NIST_ENV_ROOT ?= $(CURDIR)/target/nist
NIST_SOURCE_VAL ?=

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
	COBOLC="$(NIST_COBOLC)" \
	bash tests/nist/run_nist.sh $(if $(PROGRAM),$(MODULE) $(PROGRAM),$(or $(MODULE),--all))

## NIST 結果サマリー
nist-summary:
	NIST_ENV_ROOT="$(NIST_ENV_ROOT)" bash tests/nist/run_nist.sh --summary

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
	@echo "  make nist-run MODULE=NC - NIST 特定モジュール実行"
	@echo "  make nist-run MODULE=NC PROGRAM=NC101A - NIST 単一プログラム実行"
	@echo "  make nist-summary - NIST 結果サマリー表示"
	@echo "  make help        - このヘルプ表示"
