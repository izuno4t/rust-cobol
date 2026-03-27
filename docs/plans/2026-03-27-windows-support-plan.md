# Windows Support Plan

## Goal

`rust-cobol` を将来的に Windows でもサポート可能にする。

ここでいうサポートには、少なくとも以下を含む。

- Rust workspace が Windows でビルドできる
- 生成した C が Windows 上でコンパイルできる
- ランタイムライブラリとリンクした COBOL 実行ファイルが Windows で動く
- 継続的に Windows 上で回帰検証できる

## Current status

現時点では Windows 対応は保留としている。

理由:

- Linux/macOS と比べて C コンパイラ・リンクオプション・実行ファイル形式の差分がある
- E2E テストや NIST/benchmark には Unix パス前提の箇所が残っている
- まずは Linux/macOS と x86 Linux runtime 検証を優先したい

## Required work

### 1. Build and link support

Windows でまず確認すべき項目:

- `cargo build --workspace`
- `cargo test --workspace --lib`
- `cobol-driver` による C 生成
- 生成 C のコンパイル
- ランタイム静的ライブラリとのリンク

主な論点:

- Windows では実行ファイル拡張子が `.exe`
- Linux 向けの `-ldl` や `-lpthread` がそのまま使えない可能性がある
- 使用する C toolchain を固定する必要がある
  - 候補: MSYS2 UCRT64 + GCC/Clang

### 2. CI path

最小の継続検証は GitHub Actions の `windows-latest` で行う。

想定構成:

- shell: MSYS2 UCRT64
- Rust, GCC/Clang, `make` を同一環境で揃える
- 最初は smoke test のみ

段階案:

1. workspace build
2. unit test
3. simple COBOL smoke compile/run
4. 軽量 E2E
5. 余裕があれば benchmark / NIST の一部

### 3. Runtime smoke test

Windows 対応の第一段階では、まず最小の smoke test を用意する。

要件:

- ごく短い COBOL プログラムをコンパイルできる
- 生成した Windows バイナリが起動し、期待文字列を出力する

この smoke が通れば、少なくとも
「driver -> C codegen -> native compile -> runtime execution」の
最小経路が成立したと判断できる。

### 4. Test portability fixes

既存テストには Unix 前提の記述がある。

代表例:

- `/tmp/...` 固定パス
- シェルや実行環境を Unix とみなしたテスト
- 実行ファイル名に拡張子が無い前提

必要な整理:

- 一時ファイルは `tempfile` ベースに寄せる
- OS ごとにファイルパスを切り替える
- `.exe` 有無をヘルパー関数で吸収する

### 5. NIST and benchmark scope

Windows でも最終的には NIST と benchmark を回したいが、
最初から全面対応は行わない。

優先順:

1. Windows smoke
2. 軽量 E2E
3. benchmark の一部
4. NIST の一部
5. 必要性が高ければ NIST 全量

## Recommended environment

Windows 実行環境の第一候補:

- GitHub Actions `windows-latest`
- MSYS2 UCRT64

理由:

- macOS ホストに Windows 環境を持ち込まずに検証できる
- Rust と C toolchain を同じシェル環境で揃えやすい
- 継続的な回帰検証にそのまま使える

## Deferred decision

以下は Windows 対応を再開するまで保留:

- Windows 専用 workflow の追加
- Windows smoke script の追加
- Windows 固有の code path 追加
- README への Windows 対応明記

## Exit criteria

Windows 対応を「着手済み」と見なす条件:

1. Windows CI で workspace build が通る
2. Windows smoke compile/run が通る
3. 最低限の E2E が通る
4. 必要な OS 差分がドキュメント化されている
