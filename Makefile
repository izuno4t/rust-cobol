# COBOL Compiler - Makefile
# Rust workspace 用のビルド/テスト/リント操作をまとめたもの

CARGO := cargo
BINARY := cobolc
INSTALL_DIR := $(HOME)/.cargo/bin

.PHONY: all build release test test-unit test-e2e lint fmt check clippy clean install uninstall example spellcheck nist nist-summary help

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

## NIST CCVS 85 テスト実行 (モジュール指定: make nist MODULE=NC)
nist: release
	COBOLC="$(CARGO) run --release --package cobol-driver --" tests/nist/run_nist.sh $(or $(MODULE),--all)

## NIST 結果サマリー
nist-summary:
	tests/nist/run_nist.sh --summary

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
	@echo "  make nist        - NIST CCVS 85 全モジュール実行"
	@echo "  make nist MODULE=NC - NIST 特定モジュール実行"
	@echo "  make nist-summary - NIST 結果サマリー表示"
	@echo "  make help        - このヘルプ表示"
