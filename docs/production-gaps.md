# 製品レベルまでの残課題一覧

更新日: 2026-05-08

過去の大半の項目は実装済みになったため、この文書は「まだ残っている課題」だけに整理した。
JSON/XML 文、`RENAMES`、日付関数、数学関数、`PERFORM THRU`、`GOBACK`、
基本的な Report Writer、`VALIDATE`、`SCREEN SECTION`、`DECLARATIVES` など、
以前の大きな欠落はここから外している。

## 判定基準

- `高` - 仕様上存在するが、実行時に未完成またはスタブが残る
- `中` - 一部実装済みだが、E2E 検証や機能範囲が不足している
- `低` - 実用上の優先度は低いが、標準対応としては未完了

## 中

### 1. Report Writer の高度な帳票制御

- 未対応:
  制御脚書き/頭書き、ページ遷移時の詳細な出力制御
- 必要な対応:
  report group 種別ごとのページング規則を E2E で拡充する

### 2. VALIDATE の標準詳細条件

- 未対応:
  COBOL 2014 の文脈依存検証、複合グループ制約、より広いカテゴリ検証
- 必要な対応:
  グループ単位の検証規則と標準上の詳細な適用条件を E2E で拡充する

### 3. SCREEN SECTION のフォーム編集互換性

- 未対応:
  端末上でのインライン編集、入力マスク、ファンクションキー、カーソル移動
- 必要な対応:
  端末固有の入力制御を切り出し、フォーム編集操作を E2E で拡充する

### 4. DECLARATIVES / USE AFTER EXCEPTION の操作別網羅

- 未対応:
  `READ` / `REWRITE` / `DELETE` / `START` の操作別例外条件と分類
- 必要な対応:
  ファイル操作ごとの例外発火条件と、明示的な `INVALID KEY` / `AT END` 句との優先関係を E2E で拡充する

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

### 6. COMMUNICATION SECTION は production-ready ではない

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
