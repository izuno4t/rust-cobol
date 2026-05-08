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

1. `sema` の成果物が後段の正規入力として使われ始めたが、
   typed HIR にはまだ到達していない
2. `cobol-hir` が IR 定義と lowering 実装を同時に抱えている

2026-05-08 時点で、driver の通常経路は `lower_analyzed_to_hir` を使い、
semantic analysis の成功を HIR lowering の明示的な前提にした。
さらに `cobol-hir::lower` は非公開 module とし、lowering API は
crate root からの re-export に限定した。
また `AnalysisResult::resolve_data_reference` を追加し、
`lower_analyzed_to_hir` は一部の procedure 内データ参照を
semantic analysis の symbol table で検証するようになった。

---

## 良い点

- Cargo workspace によるクレート分割が明確で、共通基盤と各コンパイル段階が分離されている
- `cobol-preprocessor` を独立クレートとして分けており、COPY/REPLACE を lexer から分離できている
- `cobol-runtime` を独立させており、生成コードとの ABI 境界を意識した構成になっている
- `cobol-driver` がコンパイルパイプラインのオーケストレーションに集中しており、入口が明確

---

## 指摘事項

### 1. `sema` の結果が `HIR` に部分的に接続された

**重要度:** 高

`cobol-driver` の通常経路では、意味解析を実行した後、
`lower_analyzed_to_hir(&program, &result)` を経由して HIR を生成する。
これにより、semantic analysis が error-free であることは
HIR lowering の入口契約になった。

- 参照: `crates/cobol-driver/src/main.rs`
- フェーズ3: `SemanticAnalyzer::analyze(&program)`
- フェーズ4: `lower_analyzed_to_hir(&program, &result)`

ただし現状では、`AnalysisResult` の詳細な名前解決・型解決情報を
HIR の正規データモデルとして使い切っているわけではない。
そのため、`HIR lowering` 側で AST 由来の曖昧さを補修する責務はまだ残っている。

実際に `cobol-hir/src/lower.rs` には以下のような後処理が入っている。

- `FILE STATUS` / `ASSIGN TO` / `DECLARATIVES` の抽出
- `WRITE/REWRITE` の対象解決
- `OCCURS` 次元に基づく添字解釈の補正

今回の改善で、`sema` 成功と一部の解決済みデータ参照を
後段入力の前提にする足場は入った。
しかし `typed HIR` またはそれに準ずる後段入力へ昇格させない限り、
意味解析と lowering の責務境界はまだ崩れやすい。

**推奨:**

- 中長期的には `sema -> typed HIR` の流れに寄せる
- 少なくとも「後段が信頼すべき正規情報」を AST ではなく `sema` の成果物に寄せる
- `AnalysisResult` に後段が参照できる解決済み情報をさらに段階的に追加する

### 2. `cobol-hir` が IR 定義と lowering 実装を同じ crate に持っている

**重要度:** 中

`cobol-hir` は IR 型定義クレートであると同時に、AST からの lowering と複数の後処理ロジックを持っている。
今の規模では問題ないが、今後 `typed HIR`、最適化、CFG、分析支援を追加すると責務が膨らみやすい。

2026-05-08 時点では、`lower` module を非公開化し、外部利用者は
crate root の `lower_analyzed_to_hir` / `lower_to_hir` だけを使う形にした。
これにより実装詳細の直接利用は防いだが、crate 分割そのものはまだ行っていない。

**推奨:**

- 将来的に `hir-ir` と `hir-lower` 相当へ分離する余地を残す
- 直近では、`lower.rs` が意味解析の代替責務を持ち始めないよう監視する
- 公開 API は crate root に限定し、`lower` module の再公開を避ける

---

## 優先順位

1. `sema` の成果物を後段の正規入力へ接続する範囲を広げる
2. `HIR` 定義と lowering 実装を crate 分割する判断基準を決める
3. lowering が意味解析の代替責務をさらに抱えないよう、境界テストを追加する

---

## 当面のアクション

- `typed HIR` 導入の可否、または `AnalysisResult` の追加拡張方針を設計する
- `lower_to_hir` が必要とする sema 成果物を洗い出し、`AnalysisResult` 経由へ寄せる
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
