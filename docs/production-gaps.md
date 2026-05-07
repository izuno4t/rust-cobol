# 製品レベルまでの残課題一覧

更新日: 2026-03-27

過去の大半の項目は実装済みになったため、この文書は「まだ残っている課題」だけに整理した。
JSON/XML 文、`RENAMES`、日付関数、数学関数、`PERFORM THRU`、`GOBACK` など、以前の大きな欠落はここから外している。

## 判定基準

- `高` - 仕様上存在するが、実行時に未完成またはスタブが残る
- `中` - 一部実装済みだが、E2E 検証や機能範囲が不足している
- `低` - 実用上の優先度は低いが、標準対応としては未完了

## 高

### 1. Report Writer はまだスタブ

- 状況:
  `INITIATE` / `GENERATE` / `TERMINATE` は codegen されるが、
  `crates/cobol-codegen/src/stmt.rs` ではコメント出力ベースのスタブ
- 影響:
  `REPORT SECTION` を使う本格的なレポート帳票処理はまだ実行できない
- 必要な対応:
  `REPORT SECTION` の専用 HIR/runtime 設計と実処理の実装

## 中

### 2. VALIDATE の広い制約検証は限定的

- 状況:
  `VALIDATE` は PICTURE 由来の基本的な numeric storage validation を実行する
- 制約:
  `VALUE` / 条件名 / 文脈依存の検証など、COBOL 2014 の広い検証規則はまだ限定的
- 必要な対応:
  追加のデータ制約を HIR/runtime に渡し、E2E テストで互換性を確認する

### 3. SCREEN SECTION は基本表示中心

- 状況:
  `SCREEN SECTION` 用の parser / HIR / codegen / runtime は存在し、
  ANSI エスケープによる位置決め・反転・ハイライト・画面クリアは実装済み
- 制約:
  フル機能の画面フォーム処理としては未完成で、対話的 `ACCEPT` を含む広い互換性は未確認
- 必要な対応:
  対話入力を含む E2E テスト拡充と、必要なら端末制御層の強化

### 4. DECLARATIVES / USE AFTER EXCEPTION の実運用検証不足

- 状況:
  parser では `DECLARATIVES` を解釈でき、HIR lowering と codegen 側にも
  ディスパッチ処理がある
- 制約:
  実ファイル I/O エラーを起点にしたネイティブ E2E 検証はまだ薄い
- 必要な対応:
  `OPEN` / `READ` / `WRITE` 失敗時に宣言節が正しく発火する統合テストを追加

### 5. OOP / later-standard advanced features の E2E カバレッジ不足

- 対象:
  `CLASS-ID`、`METHOD-ID`、`INTERFACE-ID`、`INVOKE`、`FUNCTION-ID`、`TYPEDEF`
- 状況:
  IR や codegen 側の実装は入っているが、機能ごとのネイティブ実行保証はまだ薄い
- 影響:
  パースや C 生成は通っても、実運用レベルの信頼性はまだ十分に示せていない
- 必要な対応:
  小さな codegen テストではなく、実行まで含む E2E サンプルを揃える

### 6. SORT ... INPUT PROCEDURE / OUTPUT PROCEDURE の実運用保証不足

- 状況:
  基本の `SORT` / `MERGE` 支援はあるが、手続き型ソートはまだ部分対応扱い
- 影響:
  `RELEASE` / `RETURN` を使う古典的なソート処理で、互換性に不安が残る
- 必要な対応:
  `INPUT PROCEDURE` / `OUTPUT PROCEDURE` を使う E2E テストの拡充と、
  必要ならランタイム連携の補強

### 7. COMMUNICATION SECTION は production-ready ではない

- 状況:
  parser / AST / runtime の土台はあるが、通常利用での検証実績はまだ限定的
- 影響:
  `SEND` / `RECEIVE` 系の古い通信機能を使うプログラムは、互換性判断が難しい
- 必要な対応:
  実行モデルの整理、仕様境界の明文化、E2E テスト整備

## 低

### 8. 厳密な標準モード切替が CLI に露出していない

- 状況:
  コードベースには `Cobol85` / `Cobol2002` / `Cobol2014` / `Cobol2023` の内部 enum がある
  一方で CLI には `--standard` オプションがない
- 影響:
  「どの標準までを許可するか」をユーザーが明示的に制御できない
- 必要な対応:
  標準モード切替を公開するか、当面は混在サポート方針を明文化する

## メモ

- この文書は「残課題一覧」なので、完了済み項目は残さない
- 完了済みの対応状況は
  [docs/cobol-standards.md](./cobol-standards.md) と
  [docs/user-guide.md](./user-guide.md) に反映する
