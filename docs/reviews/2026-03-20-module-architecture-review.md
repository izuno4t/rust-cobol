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
2. 公開されているパイプライン説明が実装上の責務境界とずれている

このうち優先度が高いのは 1 だが、2 についてはコード変更より先に設計記述を現状実装へ合わせて更新すべきである。

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

### 2. パイプライン説明が実装とずれている

**重要度:** 中

README では主経路が次のように説明されている。

`Source → Lexer → Parser → Sema → HIR → C codegen → clang → native binary`

しかし実装では、lexer の前に `preprocessor` が独立フェーズとして存在する。
また `cobol-mir` は README に正式な構成要素として掲載されている一方で、
実装上は未使用の予約クレートである。

このずれは実装バグではないが、設計理解のノイズになる。

**推奨:**

- README と設計文書に `Preprocessor` を正式フェーズとして明記する
- `MIR` は「将来用で現状未使用」と明記する
- 現状の責務境界を説明し、将来構想と現在実装を混同させない

**優先判断:**

この論点はコード変更より先に、設計記述を現状実装へ合わせて更新するのが先である。

### 3. `codegen` とツールチェーン実行責務が分離し切れていない

**重要度:** 中

`cobol-codegen` は C コード生成だけでなく、C コンパイラ実行機能も公開している。
一方で runtime ライブラリの探索は `cobol-driver` 側にある。

このため、バックエンド責務が次の2箇所に分散している。

- `cobol-codegen`: C ソース生成、C コンパイル実行
- `cobol-driver`: runtime ライブラリ探索、実行環境の組み立て

現時点では運用可能だが、バックエンド差し替えやビルド設定の抽象化を行う段階で境界が曖昧になりやすい。

**推奨:**

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
2. README / 設計文書を現状実装に合わせて修正する
3. `codegen` とツールチェーン実行の責務分離を整理する
4. `HIR` 定義と lowering 実装の将来的な分離方針を決める

---

## 当面のアクション

- まず文書を更新し、現状パイプラインを正しく説明する
- 次に `typed HIR` 導入の可否、または `AnalysisResult` の拡張方針を設計する
- その後に `codegen` / `driver` の責務分離を判断する

---

## 参照箇所

- `Cargo.toml`
- `README.md`
- `crates/cobol-driver/src/main.rs`
- `crates/cobol-sema/src/analyzer.rs`
- `crates/cobol-hir/src/lower.rs`
- `crates/cobol-codegen/Cargo.toml`
- `crates/cobol-mir/src/lib.rs`
