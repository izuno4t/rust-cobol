# モジュール構成レビュー

作成日: 2026-03-20
対象: `rust-cobol` ワークスペース全体
観点: クレート分割、依存方向、責務境界、実装と設計記述の整合

---

## 結論

全体の分割は概ね素直で、
`common / diagnostics / preprocessor / lexer / parser / sema / hir /
codegen / runtime / driver` という責務分離は理解しやすい。

一方で、現状の主要な構造課題は次の2点である。

1. `sema` の成果物が後段の正規入力になっていない
2. `cobol-hir` が IR 定義と lowering 実装を同時に抱えている

2026-05-08 時点で、公開パイプライン説明のずれと `codegen` /
外部ツールチェーン実行責務の混在は改善済みである。

---

## 良い点

- Cargo workspace によるクレート分割が明確で、共通基盤と各コンパイル段階が分離されている
- `cobol-preprocessor` を独立クレートとして分けており、COPY/REPLACE を lexer から分離できている
- `cobol-runtime` を独立させており、生成コードとの ABI 境界を意識した構成になっている
- `cobol-driver` がコンパイルパイプラインのオーケストレーションに集中しており、入口が明確

---

## 指摘事項

### 1. `sema` の結果が `HIR` に接続されていない

**重要度:** 高

`cobol-driver` では意味解析を実行した後も、`HIR` 生成は未型付けの AST から直接行っている。

- 参照: `crates/cobol-driver/src/main.rs`
- フェーズ3: `SemanticAnalyzer::analyze(&program)`
- フェーズ4: `lower_to_hir(&program)`

このため、名前解決・型解決・制約解決の結果が後段の正規データモデルになっていない。
その結果、`HIR lowering` 側で AST 由来の曖昧さを補修する責務を持ち込みやすい。

実際に `cobol-hir/src/lower.rs` には以下のような後処理が入っている。

- `FILE STATUS` / `ASSIGN TO` / `DECLARATIVES` の抽出
- `WRITE/REWRITE` の対象解決
- `OCCURS` 次元に基づく添字解釈の補正

`sema` の成果物を `typed HIR` またはそれに準ずる後段入力へ昇格させない限り、
意味解析と lowering の責務境界は今後も崩れやすい。

**推奨:**

- 中長期的には `sema -> typed HIR` の流れに寄せる
- 少なくとも「後段が信頼すべき正規情報」を AST ではなく `sema` の成果物に寄せる

### 2. パイプライン説明が実装とずれていた

**状態:** 解消済み

README の主経路は現在、実装どおり次の流れを明記している。

```text
Source → Preprocessor → Lexer → Parser → Sema → HIR → C codegen → C toolchain → native binary
```

また `cobol-mir` は「現在の主パイプラインでは未使用の placeholder」として
明記されている。

このため、本指摘は現時点では未解決課題ではない。

**対応済み:**

- README と設計文書に `Preprocessor` を正式フェーズとして明記する
- `MIR` は「将来用で現状未使用」と明記する
- 現状の責務境界を説明し、将来構想と現在実装を混同させない

### 3. `codegen` とツールチェーン実行責務が分離し切れていなかった

**状態:** 解消済み

`cobol-codegen` は現在、`HIR -> C 文字列生成` に公開 API を絞っている。
C コンパイラ起動、runtime static library 解決、リンク戦略は
`cobol-driver::toolchain` に移した。

このため、バックエンド責務は次のように分かれている。

- `cobol-codegen`: C ソース生成
- `cobol-driver::toolchain`: C コンパイラ起動、runtime library 解決、リンク

本指摘は、現時点では未解決課題ではない。

**対応済み:**

- `codegen` を「IR -> C 文字列生成」に寄せる
- 外部コンパイラ起動とリンク戦略は `driver` か別の `toolchain` 層へ寄せる

### 4. `cobol-hir` が IR 定義と lowering 実装を同時に抱えている

**重要度:** 中

`cobol-hir` は IR 型定義クレートであると同時に、AST からの lowering と複数の後処理ロジックを持っている。
今の規模では問題ないが、今後 `typed HIR`、最適化、CFG、分析支援を追加すると責務が膨らみやすい。

**推奨:**

- 将来的に `hir-ir` と `hir-lower` 相当へ分離する余地を残す
- 直近では、`lower.rs` が意味解析の代替責務を持ち始めないよう監視する

---

## 優先順位

1. `sema` の成果物を後段の正規入力へ接続する設計を検討する
2. `HIR` 定義と lowering 実装の将来的な分離方針を決める
3. lowering が意味解析の代替責務をさらに抱えないよう、境界テストを追加する

---

## 当面のアクション

- `typed HIR` 導入の可否、または `AnalysisResult` の拡張方針を設計する
- `lower_to_hir` が必要とする sema 成果物を洗い出す
- `cobol-hir` を将来 `hir-ir` / `hir-lower` 相当に分ける判断基準を定義する

---

## 参照箇所

- `Cargo.toml`
- `README.md`
- `crates/cobol-driver/src/main.rs`
- `crates/cobol-sema/src/analyzer.rs`
- `crates/cobol-hir/src/lower.rs`
- `crates/cobol-codegen/Cargo.toml`
- `crates/cobol-mir/src/lib.rs`
