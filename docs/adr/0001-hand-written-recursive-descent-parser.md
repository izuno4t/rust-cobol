# ADR-0001: Hand-Written Recursive Descent Parser

- **Status**: Accepted
- **Date**: 2026-03-20
- **Deciders**: Project maintainers

## Context

COBOL compiler の Parser 実装方式を選定する必要がある。候補は以下の通り:

1. **手書き再帰下降パーサー** — コードで直接パーサーロジックを記述する
2. **LL(\*) 系パーサージェネレータ** — ANTLR4（Adaptive LL(\*)）
3. **LR(1) 系パーサージェネレータ** — lalrpop（LR(1)、Rust ネイティブ）
4. **PEG 系パーサージェネレータ** — pest（Parsing Expression Grammar、Rust ネイティブ）

なお、レキサーについては方式によらず手書きとする。COBOL の Fixed format（カラム依存レイアウト）や PICTURE 句（`PIC 9(3)V99`）の特殊トークン化はレキサー／プリプロセッサの設計課題であり、パーサー方式の選定とは独立している。実際に ANTLR4 は `TokenSource` インターフェース経由で外部レキサーを受け入れ可能であり、lalrpop も `extern` ブロックで外部レキサーを公式にサポートしている。

以上を前提として、**パーサー層の実装方式**に焦点を当てて比較する。

## Decision

**手書き再帰下降パーサーを採用する。**

決定的な理由は、COBOL の文脈依存的な曖昧性解消をパーサー内部で柔軟に処理する必要があること、および本プロジェクトの `DiagnosticReporter` ベースのエラー回復方式と手書きパーサーの親和性が高いことの2点である。

## Rationale

### 1. COBOL 文法の文脈依存的な曖昧性解消

COBOL の文法は文脈依存性が極めて高く、パーサー層で動的な判断が求められる場面が多い。

- **コンテキスト依存キーワード**: IBM Enterprise COBOL の仕様では[コンテキスト依存ワード](https://www.ibm.com/docs/en/cobol-zos/6.4.0?topic=appendixes-context-sensitive-words)が定義されており、同一トークンが文脈により予約語にも識別子にもなる。300語以上の予約語に対してこれを処理する必要がある
- **構文上の曖昧性**: `COMPUTE` 文の `=` と条件式の `=` の区別、`OF`/`IN` による修飾名の解決など、パーサーが現在の解析コンテキストを保持して判断する必要がある
- **手書きパーサーの優位性**: 再帰下降では、各パース関数が呼び出しコンテキストを自然に保持しており、「この文脈では `MOVE` はキーワード、この文脈では識別子」といった判断を関数単位で記述できる

各候補方式でこの問題をどう扱うかは「Alternatives Considered」で個別に比較する。

### 2. エラー回復の制御

本プロジェクトでは `DiagnosticReporter` にエラーを蓄積して解析を継続する方式を採用している。手書きパーサーでは、文ごと・句ごとに同期ポイントを配置し、COBOL の DIVISION/SECTION/PARAGRAPH 境界に沿ったエラー回復を細かく制御できる。

各候補方式のエラー回復能力は異なるため、詳細は「Alternatives Considered」で個別に評価する。

## Alternatives Considered

### A. ANTLR4（Adaptive LL(\*)）

ANTLR4 は Adaptive LL(\*) アルゴリズムを採用しており、従来の LL(k) より広いクラスの文法を実用的に扱える（ただし全ての文脈自由文法を扱えるわけではなく、間接左再帰など制約は残る。詳細は Parr et al. 2014 を参照）。COBOL パーサーの実績として [ProLeap](https://github.com/uwol/proleap-cobol-parser)（Java, COBOL 85, NIST テスト合格実績あり）が存在する。

**Pros:**

- 曖昧な入力に対して文法定義順で代替規則を選択する暗黙的解決に加え、`AmbiguityInfo` による曖昧性検出が可能
- 自動エラー回復が組み込み（single token deletion/insertion, resynchronization）。`ANTLRErrorStrategy` インターフェースによりカスタマイズも可能
- 外部レキサーを `TokenSource` 経由で統合可能
- ProLeap や [grammars-v4 の Cobol85.g4](https://github.com/antlr/grammars-v4/blob/master/cobol85/Cobol85.g4) など、COBOL 85 の既存文法定義があり再利用可能

**Cons:**

- **公式 Rust ターゲットが存在しない**: コミュニティ実装（[antlr4rust](https://github.com/rrevenantt/antlr4rust)）はあるが、公式サポートではなく API 安定性が保証されていない。本プロジェクトの長期的な保守を考えると、非公式バインディングへの依存はリスクとなる
- 文脈依存キーワードの処理は文法定義だけでは完結しない。実際に grammars-v4 の Cobol85.g4 は semantic predicate（`{...}?`）や lexer mode を使い分けて文脈依存トークンを処理しており、文法ルールと手続き的コードが混在する構造になっている。また cobolg プロジェクトは Fixed format 対応のために入力ストリームへの人工的な文字挿入を行っている（[rslemos/cobolg README](https://github.com/rslemos/cobolg)）
- Java ツールチェーンへの依存（文法からのコード生成時）

**不採用理由**: 公式 Rust ターゲットが存在せず、非公式実装への依存が長期的なリスクとなる。また、文脈依存キーワードの処理に predicate や lexer mode を多用する必要があり、文法定義の宣言性というパーサージェネレータの利点が減殺される。

### B. lalrpop（LR(1)）

Rust ネイティブの LR(1) パーサージェネレータ。Rust コミュニティで広く利用されており、外部レキサー統合を公式にサポートしている。

**Pros:**

- **LR(1) 衝突を文法生成時に検出・拒否**する。曖昧性が暗黙的に解決されることはなく、文法の正確性を静的に保証できる
- `!` トークンによるエラー回復ポイントを文法内で明示的に定義可能
- 外部レキサーを `extern` ブロックで公式サポート。`Iterator<Item = Result<(Loc, Tok, Loc), Error>>` を受け付ける
- Rust ネイティブで安定しており、ビルドプロセスに自然に統合できる

**Cons:**

- **LR(1) の制約**: COBOL の文脈依存的な曖昧性は LR(1) 文法では shift/reduce・reduce/reduce 衝突として顕在化する。これを解決するには文法の大幅な書き換えか、レキサー側でのコンテキスト依存トークン化が必要になる
- エラー回復は明示的な `!` 配置に依存する。COBOL の多様な文構造に対して網羅的に回復ポイントを定義する必要があり、回復品質を手書きパーサーと同等にするには相当のコストがかかる
- LR(1) ステートテーブルの生成が COBOL 規模の文法で実用的な時間に収まるか未検証

**不採用理由**: LR(1) の文法クラスに COBOL の曖昧性を収めるコストが高い。文法の正確性を静的に保証できる点は魅力的だが、それを実現するための文法書き換えが本質的な複雑さを増す。

### C. pest（PEG）

Rust ネイティブの PEG パーサージェネレータ。宣言的な `.pest` 文法ファイルと `#[derive(Parser)]` による自動生成が特徴。

**Pros:**

- PEG の順序付き選択（`/` 演算子）により文法に曖昧性が存在しない（最初にマッチした代替が常に選択される）
- Rust ネイティブで安定しており、ビルドプロセスに自然に統合できる

**Cons:**

- **外部レキサーをサポートしていない**（[Issue #580](https://github.com/pest-parser/pest/issues/580) で要望されているが未実装、"separating scanner and parser would require substantial changes" とコメントされている）。本プロジェクトの手書きレキサーと統合できない
- PEG の順序付き選択は「曖昧性がない」のではなく「暗黙的に最初の選択肢が優先される」ことを意味する。COBOL のように文脈に応じて異なる選択肢が正しい場合、順序だけでは正しい解析結果を保証できない
- エラー回復メカニズムが限定的（[Issue #467](https://github.com/pest-parser/pest/issues/467) で改善が議論中）

**不採用理由**: 外部レキサーとの統合不可が致命的。加えて PEG の順序付き選択は COBOL の文脈依存的な曖昧性解消と相性が悪い。

### 方式間の比較まとめ

| 評価軸 | 手書き再帰下降 | ANTLR4 (LL(\*)) | lalrpop (LR(1)) | pest (PEG) |
|--------|---------------|-----------------|-----------------|------------|
| 文脈依存キーワード | 関数単位で自然に処理 | predicate で対応可能だが文法が肥大化 | 文法書き換えが必要 | 順序選択では不十分 |
| エラー回復の細かさ | 完全に自由 | カスタマイズ可能だが自動回復がベース | `!` 配置で明示的に定義 | 限定的 |
| 外部レキサー統合 | 不要（一体設計） | `TokenSource` 経由で可能 | `extern` で公式サポート | 不可 |
| Rust 公式サポート | ネイティブ（依存なし） | 非公式のみ | 公式（Rust ネイティブ） | 公式（Rust ネイティブ） |

## Consequences

**Positive:**

- COBOL の文脈依存的な曖昧性をパーサー関数内で直接的に解消できる
- `DiagnosticReporter` と統合したエラー回復戦略を文・句・DIVISION 単位で制御できる
- 外部ツールへの依存がなく、ビルドプロセスがシンプル
- パフォーマンスの直接制御が可能

**Negative:**

- パーサーのコード量が多くなる（文法定義ファイルに比べて）
- 文法変更時の修正が手動になる（自動再生成はできない）
- 文法のフォーマルな曖昧性検出・静的検証ができない

## References

- Parr, Harwell, Fisher. ["Adaptive LL(\*) Parsing: The Power of Dynamic Analysis"](https://www.antlr.org/papers/allstar-techreport.pdf), 2014 — ANTLR4 のアルゴリズム論文
- [ProLeap COBOL Parser](https://github.com/uwol/proleap-cobol-parser) — ANTLR4 による COBOL 85 パーサー実装（NIST テスト合格実績）
- [ANTLR grammars-v4 Cobol85.g4](https://github.com/antlr/grammars-v4/blob/master/cobol85/Cobol85.g4) — ANTLR4 用 COBOL 85 文法定義（84,396 bytes）
- [lalrpop GitHub](https://github.com/lalrpop/lalrpop) — Rust ネイティブ LR(1) パーサージェネレータ
- [pest GitHub](https://github.com/pest-parser/pest) — Rust ネイティブ PEG パーサージェネレータ
- [GnuCOBOL Feature Request #371](https://sourceforge.net/p/gnucobol/feature-requests/371/) — GnuCOBOL parser.y（20,605 行）の保守性課題に関する議論
- [OCamlPro: Fixing and Optimizing GnuCOBOL](https://ocamlpro.com/blog/2024_04_30_fixing_and_optimizing_gnucobol/) — GnuCOBOL のパーサー規模と最適化に関する分析
- [rslemos/cobolg](https://github.com/rslemos/cobolg) — ANTLR4 で COBOL を扱う際の Fixed format の課題を記録
- [IBM Enterprise COBOL: Context-Sensitive Words](https://www.ibm.com/docs/en/cobol-zos/6.4.0?topic=appendixes-context-sensitive-words) — COBOL のコンテキスト依存キーワード仕様
- Robert Nystrom, "Crafting Interpreters" — 手書き再帰下降パーサーの設計パターン
