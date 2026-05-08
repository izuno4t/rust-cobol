# 製品レベルまでの残課題一覧

更新日: 2026-05-08

過去の大半の項目は実装済みになったため、この文書は「まだ残っている課題」だけに整理した。
JSON/XML 文、`RENAMES`、日付関数、数学関数、`PERFORM THRU`、`GOBACK` など、以前の大きな欠落はここから外している。

## 判定基準

- `高` - 仕様上存在するが、実行時に未完成またはスタブが残る
- `中` - 一部実装済みだが、E2E 検証や機能範囲が不足している
- `低` - 実用上の優先度は低いが、標準対応としては未完了

## 中

### 1. Report Writer の帳票レイアウト対応は限定的

- 状況:
  `INITIATE` / `GENERATE` / `TERMINATE` は codegen され、
  `PAGE-COUNTER` / `LINE-COUNTER` の更新と `GENERATE` 対象行の出力を実行する。
  `DETAIL` report group の `VALUE` / `SOURCE` / `COLUMN` / `LINE` は基本的な整形出力に対応している
- 制約:
  `REPORT SECTION` の本格的な帳票機能としては、複数行グループ、集団項目の複雑な整形、
  制御脚書き/頭書き、ページ遷移時の詳細な出力制御がまだ限定的
- 必要な対応:
  report group 種別ごとの出力規則とページングを E2E で拡充する

### 2. VALIDATE の広い制約検証は限定的

- 状況:
  `VALIDATE` は PICTURE 由来の基本的な numeric storage validation を実行する
- 制約:
  `VALUE` / 条件名 / 文脈依存の検証など、COBOL 2014 の広い検証規則はまだ限定的
- 必要な対応:
  追加のデータ制約を HIR/runtime に渡し、E2E テストで互換性を確認する

### 3. SCREEN SECTION の端末編集互換性は限定的

- 状況:
  `SCREEN SECTION` 用の parser / HIR / codegen / runtime は存在し、
  ANSI エスケープによる位置決め・反転・ハイライト・画面クリアは実装済み。
  `ACCEPT screen-name` は `USING` フィールドへの標準入力取り込みまで実行できる
- 制約:
  フル機能の画面フォーム処理としては未完成で、端末上でのインライン編集、
  入力マスク、ファンクションキー、カーソル移動などの互換性はまだ限定的
- 必要な対応:
  端末固有の入力制御を切り出し、フォーム編集操作を E2E で拡充する

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
  `INVOKE` の runtime dispatch と null object 経路は実行テストされているが、
  クラス/メソッド構文全体のネイティブ実行保証はまだ薄い
- 影響:
  パースや C 生成は通っても、OOP プログラム全体の信頼性はまだ十分に示せていない
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

## メモ

- この文書は「残課題一覧」なので、完了済み項目は残さない
- 完了済みの対応状況は
  [docs/cobol-standards.md](./cobol-standards.md) と
  [docs/user-guide.md](./user-guide.md) に反映する
