# Correctness-Driven COBOL Compiler and Runtime Plan

**Goal:** NIST の compile error 解消を個別目標にせず、COBOL の意味を一貫して保持し、正しく実行できるコンパイラと実行モジュールを作る

**Primary Outcome:** `Source -> AST -> Sema -> HIR -> C codegen -> runtime`
の各段で意味が失われず、生成コードと runtime が同じ実行意味を共有する

**Scope:** `cobol-hir`, `cobol-codegen`, `cobol-runtime` を中心に、名前解決、制御フロー、データ表現、実行意味の責務を整理し直す

**Non-Goal:** 特定の NIST 失敗ケースだけを通すための局所修正

---

## 問題設定

今回観測された NIST compile error は、単なる codegen バグの集積ではない。
より本質的には、現在の実装が COBOL の意味を中間表現で十分に保持できておらず、
その不足を codegen と macro 規約で埋めようとしていることが問題である。

この欠陥は以下を引き起こす。

- compile error
- たまたま通るが意味がずれるコード生成
- nested program と top-level の不整合
- runtime と codegen の前提不一致
- 新機能追加時の破綻と回 regressions

したがって、改善目的は「いま落ちているテストを通す」ことではなく、
「COBOL の意味が途中で壊れない設計へ作り直す」ことと定義する。

---

## 成功条件

以下をすべて満たしたときに、この計画は完了とみなす。

1. 名前解決、修飾参照、添字、参照修飾、制御フロー遷移が、
   HIR 以降で文字列処理や暗黙規約に依存しない
2. codegen は意味解決をせず、解決済み HIR を C と runtime API に落とすだけになる
3. runtime は codegen の偶然の出力に依存せず、明示的なデータ/制御モデルを前提にする
4. NIST, e2e, crate 単位テストは、症状ではなく言語意味の検証として整備される
5. NIST の compile error 解消は副産物として得られる

---

## 現状の本質的欠陥

## 1. 名前解決済み参照が HIR に存在しない

現在の HIR は変数参照を概ね文字列で保持している。
このため、以下の COBOL の意味が HIR に残らない。

- `OF` / `IN` による修飾
- table 要素の参照先
- 親グループとの関係
- 参照修飾と添字の結合
- 曖昧性解消の結果

結果として、codegen 側が `sanitize_name` や macro 命名規約を使って
再解決しており、これは設計として誤っている。

## 2. 制御フローが解決済み遷移ではなく codegen 内部規約に依存している

paragraph, section, label, `GO TO`, `PERFORM`, `PERFORM THRU` の関係が
HIR で構造化されず、codegen が label map と dispatch を後付けしている。

結果として:

- nested program と top-level で実装が分岐する
- `_goto_target` や `_goto_dispatch` が内部 detail ではなく事実上の意味表現になる
- 制御フローの正しさを型や構造で検証できない

## 3. runtime と codegen の契約が暗黙的である

現在の実装は、C 側のレイアウト、データ表現、呼び出し規約、制御遷移規約の一部が
暗黙の了解で成立している。

これは次の観点で危険である。

- runtime 側が期待するデータ表現を型として明示できない
- codegen 変更が runtime 前提を壊しても検知しにくい
- COBOL の実行意味よりも、たまたま動く C の都合が優先される

---

## 基本方針

## 方針 1: HIR を「解決済み意味表現」にする

HIR は構文木の薄い写像ではなく、少なくとも codegen が意味解釈を
やり直さずに済むレベルまで解決済みであるべきである。

必要な改善:

- data item を一意に指す ID の導入
- 修飾名付き参照を `DataRef` として保持
- subscript, reference modification, slicing を同一参照モデルへ統合
- paragraph, label, section の解決済みターゲット化

例:

```rust
pub struct HirDataRef {
    pub item_id: HirItemId,
    pub subscripts: Vec<HirExpr>,
    pub refmod: Option<HirRefMod>,
}

pub enum HirTransferTarget {
    Paragraph(HirParagraphId),
    Label(HirLabelId),
}
```

## 方針 2: codegen の責務を「意味保存した変換」に限定する

codegen の役割は、解決済み HIR を C と runtime API に安全に落とすことだけにする。

やってはいけないこと:

- 変数名の再解決
- 修飾名の再解釈
- paragraph/label の再探索
- macro 命名規約に依存した曖昧性解消

やるべきこと:

- ID から一意な C 参照へ落とす
- HIR の制御遷移を C の制御構文へ写像する
- runtime 呼び出しを HIR の意味に従って選ぶ

## 方針 3: runtime を「正しい実行意味の担い手」として明示化する

runtime は単なる補助ライブラリではなく、COBOL の実行意味を支える一部である。
そのため、runtime と codegen の間には明示的な契約が必要である。

対象例:

- 数値表現と丸め規則
- MOVE/COMPARE のカテゴリ変換
- file status と I/O 状態
- 呼び出し規約
- 共有データ領域や linkage の扱い

改善方針:

- runtime が受け取るデータ表現を型/構造で固定する
- codegen 側の ad-hoc な C 式組み立てを減らす
- COBOL の意味差分を runtime API として吸収する箇所を明確化する

---

## 優先順位付き実装計画

## Phase 1: 解決済みデータ参照モデルの導入

目的:

- 名前解決を lower/HIR 側へ閉じ込める
- 修飾参照、添字、参照修飾を一つのモデルに統一する

対象:

- `crates/cobol-hir/src/hir.rs`
- `crates/cobol-hir/src/lower.rs`
- `crates/cobol-codegen/src/expr.rs`
- `crates/cobol-codegen/src/stmt.rs`

タスク:

1. `HirItemId` と `HirDataRef` を導入する
2. `QualifiedName` を文字列化せず解決済み参照へ lower する
3. `HirExpr::Variable` と `HirExpr::Subscript` を段階的に `DataRef` へ統合する
4. codegen の変数参照経路を `DataRef` のみに揃える

状態:

- 2026-04-12 完了

実施結果:

- `HirItemId` と `HirDataRef` を導入し、修飾名、添字、参照修飾を解決済み参照として保持するようにした
- lower で data item catalog を構築し、`QualifiedName` を `DataRef` へ解決してから HIR を生成するようにした
- `MOVE` 対象、表示、参照修飾、条件式、decimal fast path を含む codegen の主要参照経路を `DataRef` 対応へ揃えた
- 修飾参照と参照修飾の回帰テストを追加した
- `cargo test -p cobol-hir` と `cargo test -p cobol-codegen` で確認した
- `cargo test -p cobol-driver qualified_display` で対象ケースを確認した
- `cargo test -p cobol-driver reference_modification` で対象ケースを確認した

期待効果:

- 修飾名の曖昧解消が HIR で完結する
- macro 名の偶然に依存しない
- `NC207A`, `NC246A` 系の問題が構造的に再発しなくなる

## Phase 2: 解決済み制御フローモデルの導入

目的:

- paragraph/label 遷移を codegen 内部規約から切り離す
- nested/top-level 差分をなくす

対象:

- `crates/cobol-hir/src/hir.rs`
- `crates/cobol-hir/src/lower.rs`
- `crates/cobol-codegen/src/codegen.rs`
- `crates/cobol-codegen/src/stmt.rs`
- `crates/cobol-codegen/src/context.rs`

タスク:

1. `HirParagraphId`, `HirLabelId`, `HirTransferTarget` を導入する
2. `GO TO`, `PERFORM`, `PERFORM THRU` を ID ベースで lower する
3. top-level / nested program の paragraph emission を共通化する
4. `_goto_target` / `_goto_dispatch` を内部 detail に閉じ込めるか、より単純な表現に置き換える

状態:

- 2026-04-12 完了

実施結果:

- `HirParagraphId`, `HirLabelId`, `HirTransferTarget` を導入した
- paragraph/section 遷移を解決済み target として保持するようにした
- lower で top-level paragraph と section paragraph の ID を先に計画した
- `GO TO`, `PERFORM`, `PERFORM THRU` を ID ベース target へ lower した
- `HirParagraph` に kind と section 関係を保持し、codegen の section 判定を名前規約から明示的な構造へ置き換えた
- top-level と nested program の paragraph emission を共通 helper に寄せ、同じ dispatch モデルで生成するようにした
- `cargo test -p cobol-hir` と `cargo test -p cobol-codegen` で確認した
- `cargo test -p cobol-driver test_native_go_to_paragraph` と
  `cargo test -p cobol-driver test_native_perform_thru_multiple_paragraphs` で対象ケースを確認した

期待効果:

- `_goto_dispatch` のような症状が設計上出にくくなる
- 遷移の正しさを HIR レベルで検証できる
- paragraph 周りの機能拡張が安全になる

## Phase 3: runtime 契約の明文化と整理

目的:

- codegen と runtime の暗黙契約をやめる
- 実行意味の責務分担を明示する

対象:

- `crates/cobol-runtime`
- `crates/cobol-codegen`
- 必要に応じて `docs/`

タスク:

1. runtime API が担う COBOL 意味を列挙する
2. codegen が直接 C 式で持っている意味処理を洗い出す
3. runtime に寄せるべき責務と codegen に残す責務を分ける
4. 主要なデータ表現と ABI 契約を文書化する

期待効果:

- 実行意味の一貫性が上がる
- 数値/I/O/呼び出し規約の不整合を抑止できる
- 将来の runtime 最適化や別 backend への足場になる

状態:

- 2026-04-12 完了

実施結果:

- `crates/cobol-runtime/src/abi.rs` を追加し、generated C に埋め込む runtime ABI 宣言の単一ソースを作った
- `CobolDecimal`, `CobolStringSource`, `CobolUnstringTarget`, `SortKey` を
  named ABI 型として固定した
- codegen 側の `STRING`, `UNSTRING`, `SORT` 生成で匿名 struct をやめ、
  named ABI 型を使うようにした
- `emit_runtime_declarations` の重複管理をやめ、runtime 側の ABI 定義へ集約した
- `docs/runtime-abi-contract.md` を追加し、責務分担、主要 ABI 型、
  `CALL/GOBACK`, decimal, sort/string 系の契約、未整理領域を明文化した
- `cargo test -p cobol-runtime abi::tests::test_emit_c_declarations_exposes_named_runtime_abi_types`
  で ABI 宣言の整合を確認した
- `cargo test -p cobol-codegen test_generate_runtime_abi_typedefs`,
  `test_generate_string_uses_named_runtime_descriptor`,
  `test_generate_unstring_uses_named_runtime_descriptor`
  で generated C 側の利用を確認した

## Phase 4: 正しさ中心の検証体系へ移行

目的:

- NIST を単なる件数指標ではなく、意味検証の一部として扱う
- 回 regressions を症状ではなく意味差分で検知する

対象:

- `crates/cobol-driver/tests/`
- crate 単位テスト
- `tests/nist/`

タスク:

1. NIST の代表失敗ケースを縮小再現した回帰テストを追加する
2. qualified reference, table access, nested control flow の単体/結合テストを追加する
3. runtime 契約のテストを追加する
4. 生成 C の断片ではなく、COBOL 意味の結果を検証するテストを増やす

期待効果:

- compile error が消えても意味が壊れている状態を検出できる
- NIST 依存だけでは拾えない regressions を早期に捕まえられる

---

## NIST の位置づけ

NIST CCVS は重要な外部検証資産だが、目的そのものではない。

この計画における NIST の役割は以下である。

- 実装の規格整合性を広く確認する回帰スイート
- 既存の意味欠陥を発見する入力集合
- 修正後の後方退行を検知する外部ベンチマーク

つまり、NIST の compile error 解消は成功条件の一部ではあるが、
それだけを追う実装は採用しない。

---

## 避けるべき対応

以下は今回の目的に反する。

- unqualified macro を戻して偶然通す
- nested program 側だけに label を足して通す
- `sanitize_name` ルール追加で局所的に名前を合わせる
- codegen 内でさらに条件分岐を足して意味解釈を増やす
- runtime の暗黙契約を増やす

これらは compile error を減らしても、正しい COBOL コンパイラ/実行系には近づかない。

---

## 実装時の判断基準

変更案を評価するときは、以下を優先する。

1. COBOL の意味が中間表現で保持されるか
2. 名前解決や制御フロー解決が一度だけ行われるか
3. codegen と runtime の責務境界が明確か
4. nested program と top-level で同じモデルを使えるか
5. テストが症状ではなく意味を検証しているか

---

## Verification

修正後の確認として最低限以下を実行する。

```bash
make clean test lint
bash tests/nist/run_nist.sh --all
```
