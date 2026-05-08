# COBOL 仕様調査

更新日: 2026-05-08

この文書は、`rust-cobol` の仕様判断で参照する COBOL 標準の調査結果を
まとめる。標準本文は有償公開のため、ここでは ISO、ANSI、INCITS、NIST
などの公開メタデータと標準化団体の発表から確認できる範囲を記録する。

## 結論

- 現行の国際標準は ISO/IEC 1989:2023 である。
- ISO/IEC 1989:2023 は ISO/IEC 1989:2014 を置き換える第 3 版で、
  構文と意味論を定義する。
- この文書は仕様調査の記録であり、実装が仕様を満たすことの検証結果ではない。
  実装検証は [COBOL 適合性検証計画](./cobol-conformance-verification.md) で
  テスト、判定基準、未検証範囲を追跡する。
- 次版候補は ISO/IEC CD 1989 として委員会ドラフト段階にあるため、
  実装基準として固定する対象ではない。
- `rust-cobol` は、互換性と検証資産の観点では COBOL-85 を基準にし、
  後続標準の機能は標準モードごとに段階的に扱うのが妥当である。
- NIST CCVS85 は COBOL-85 系の回帰検証資産として有用だが、
  2002 以降の機能の網羅性を示すものではない。

## 標準の位置付け

ISO/IEC 1989 は COBOL の構文と意味論を規定する国際標準である。
ISO の公開概要では、標準が規定する対象として、コンパイル単位の形、
コンパイルの効果、実行単位の効果、処理系が定義すべき要素、未定義の要素、
処理系依存の要素が挙げられている。

一方で、標準は実行コードの生成方法、リンクやバインドの時期、ロケールの提供機構、
診断メッセージやリスティングの形式、実装者文書の形式、ファイル以外の
オブジェクト共有機構までは規定しない。したがって、コンパイラ実装では
「構文・意味論として標準に従う領域」と「処理系定義として明文化する領域」を
分けて管理する必要がある。

## 版の系譜

| 標準 | 状態 | 位置付け |
| --- | --- | --- |
| ISO 1989:1985 | Withdrawn | ANSI X3.23-1985 を国際標準として採用した COBOL-85 系の基準 |
| ISO 1989:1985/Amd 1:1992 | Withdrawn | Intrinsic Function Module を追加 |
| ISO 1989:1985/Amd 2:1994 | Withdrawn | COBOL-85 系の修正・明確化 |
| ISO/IEC 1989:2002 | Withdrawn | オブジェクト指向、国際化、自由形式ソースなどを大きく拡張 |
| ISO/IEC 1989:2014 | Withdrawn | 2002 版を置き換えた第 2 版 |
| ISO/IEC 1989:2023 | Published | 現行の第 3 版 |
| ISO/IEC CD 1989 | Under development | 第 4 版候補の委員会ドラフト |

## COBOL-85 系

COBOL-85 系は ANSI X3.23-1985 / ISO 1989:1985 を中心に、1989 年の
Intrinsic Function Module と 1994 年の修正・明確化を含めて参照されることが多い。
ANSI の公開情報では、ANSI INCITS 23-1985 は COBOL プログラムの
形式と解釈を規定し、機械独立性を目的としている。

実装判断では、次の領域を COBOL-85 系の中核として扱う。

- `IDENTIFICATION`、`ENVIRONMENT`、`DATA`、`PROCEDURE` の各 division
- 固定形式ソースと行領域
- データ記述、レベル番号、`PICTURE`、`USAGE`
- 算術、転記、条件分岐、反復、手続き呼び出し、入出力などの基本 statement
- sequential、relative、indexed file などのファイル処理
- `SORT`、`MERGE`、report writer、communication などの古典的モジュール

NIST CCVS85 は COBOL-85 コンパイラ検証システムとして公開記録があり、
このリポジトリの NIST 回帰テスト方針と整合する。ただし、CCVS85 の合格は
COBOL-85 系の検証であり、2002 以降の標準機能の合格を意味しない。

## COBOL 2002

ISO/IEC 1989:2002 は ISO 1989:1985 とその amendment を置き換えた版である。
INCITS の発表では、2002 版の主な拡張として次が挙げられている。

- オブジェクト指向プログラミング機能
- 例外の検出と報告
- boolean データ型と boolean 演算
- native binary と floating-point データ型
- national character data
- 文化・ロケール適応
- 算術の移植性改善
- free-form source と library text
- portable な compiler directive と conditional compilation
- report writer の拡張
- data validation
- recursion を含む `CALL` statement の拡張
- 他言語との相互運用
- user-defined function
- screen handling
- file sharing と record locking
- ISO/IEC 10646 系の文字データ交換

この版は既存 COBOL-85 資産への互換性を維持しながら、現代的な構文、
オブジェクト指向、国際化、相互運用性を追加する節目として扱う。

## COBOL 2014

ISO/IEC 1989:2014 は ISO/IEC 1989:2002 を置き換えた第 2 版である。
ISO の公開概要上、目的と規定範囲は 2023 版と同じく COBOL の構文と意味論である。
2023 版の公開後は withdrawn となっているため、現時点で新しい実装判断の
最終基準にはしない。

`rust-cobol` では、2014 版相当の機能を扱う場合でも、現行標準との差分を
確認したうえで `Cobol2014` モードの互換性として管理する。

## COBOL 2023

ISO/IEC 1989:2023 は 2023 年 1 月に公開された現行の第 3 版である。
ISO の公開情報では 1229 ページの国際標準で、ISO/IEC JTC 1/SC 22 が
技術委員会として示されている。

INCITS の発表では、2023 版は 2014 版を置き換える技術改訂であり、
主な変更として次が挙げられている。

- `SEND` / `RECEIVE` statement による非同期メッセージング
- boolean exclusive-or 演算子
- boolean shifting 演算子
- COBOL word 長の 63 文字化
- 一定時間停止を指定できる `PERFORM` statement
- `DELETE FILE` statement
- numeric-edited item に対する `VALUE` clause の拡張と変更
- external item としての type declaration
- `USAGE` clause の `NO SIGN` phrase による unsigned packed-decimal item
- `PICTURE` clause の `EDITING` phrase による user-defined editing
- プログラム間の `EXTERNAL` attribute checking
- `PERFORM ... UNTIL EXIT` による無限ループ
- `PERFORM` statement の exception-checking format による inline exception handling
- 後方検査に対応する enhanced `INSPECT` statement
- line sequential file organization
- dynamic length elementary item の長さ設定に対応する `SET` statement
- indexed file の alternate key suppression
- `COMMIT` / `ROLLBACK` statement による任意の commit / rollback processing
- 成功した入出力 statement の warning を扱う非致命的な `EC-I-O-WARNING`

これらは現行標準の方向性を示すが、公開発表は標準本文そのものではない。
実装時には、標準本文を確認できる環境で構文、制約、例外条件、処理系定義事項を
個別に確認する必要がある。

## 次版候補

ISO は ISO/IEC CD 1989 を ISO/IEC 1989:2023 の置き換え候補として公開している。
2026-01-24 時点で committee draft consultation initiated の段階であり、
edition 4 の委員会ドラフトとして扱われている。

このため、次版候補の内容は将来互換性の調査対象にはできるが、
`rust-cobol` の既定動作や conformance 判定の基準にはしない。

## 実装方針への落とし込み

### 標準モード

- `Cobol85`: 既定の互換性基準。NIST CCVS85 と既存資産の互換性を重視する。
- `Cobol2002`: free-form source、national data、OO、関数、XML などを
  COBOL-85 との差分として扱う。
- `Cobol2014`: 2014 版互換を維持する必要がある場合の中間モードとして扱う。
- `Cobol2023`: 現行標準の追加機能を受け入れるモードとして扱う。

### 処理系定義として文書化すべき領域

ISO/IEC 1989 は処理系依存や処理系定義の要素を許容する。
次の領域は実装だけでなく、ユーザー向け文書にも明記する。

- diagnostic message の形式と終了コード
- source format の既定値と `COPY` 探索順
- numeric storage、丸め、overflow、例外条件
- file organization、record locking、commit / rollback の対応範囲
- locale、文字集合、national data、encoding
- runtime module のリンク方式と ABI
- non-standard extension の扱い

### 検証方針

- COBOL-85 系の回帰検証は NIST CCVS85 を主軸にする。
- 後続標準の機能は、標準版ごとの構文テスト、意味論テスト、実行テストを
  個別に追加する。
- `Partial` または `Experimental` な機能は、対応範囲と非対応範囲を
  `docs/cobol-standards.md` とユーザー向け文書に反映する。
- 標準本文を確認できない範囲は、確認済み公開情報に基づく推定として扱い、
  conformance claim に使わない。

### ベンダー資料の扱い

日立の COBOL2002 マニュアル一覧には、`COBOL2002 言語 標準仕様編`、
`COBOL2002 言語 拡張仕様編`、ユーザーズガイド、XML 連携機能ガイド、
メッセージ一覧などが含まれる。これは日本語で COBOL2002 系の実装挙動や
ベンダー拡張を確認できる有用な資料である。

ただし、日立資料は製品マニュアルであり、ISO/IEC 1989 の代替規格ではない。
標準準拠の根拠には ISO/IEC 1989 と標準化団体の情報を優先し、日立資料は
次の用途に限定して参照する。

- 日本語で COBOL2002 系の構文説明を確認する
- ベンダー拡張と標準機能を切り分ける
- 実装済み機能のユーザー向け説明や診断メッセージの参考にする
- 他処理系との互換性差分を調べる

## 判断記録

- 既存の `docs/cobol-standards.md` は実装状況の一覧として維持し、
  本文書は標準仕様そのものの調査記録として分離した。
- 実装基準は COBOL-85 first とし、2023 版機能は opt-in の標準モードで
  扱う方針が妥当と判断した。
- ISO/IEC CD 1989 はドラフト段階のため、将来調査対象に留める。
- 標準本文は有償で直接確認できていないため、本文の詳細条項番号や
  完全な構文定義は記載しない。

## 参考資料

| 資料 | 信頼度 | 確認内容 |
| --- | --- | --- |
| [ISO/IEC 1989:2023](https://www.iso.org/standard/74527.html) | official | 現行版、公開日、規定範囲、ライフサイクル、次版候補 |
| [ISO/IEC CD 1989](https://www.iso.org/standard/87736.html) | official | 次版候補の委員会ドラフト状態 |
| [ISO/IEC 1989:2014](https://www.iso.org/standard/51416.html) | official | 2014 版の状態、規定範囲、2023 版への置換 |
| [ISO/IEC 1989:2002](https://www.iso.org/standard/28805.html) | official | 2002 版の状態、1985 版からの置換 |
| [ISO 1989:1985](https://www.iso.org/standard/6724.html) | official | COBOL-85 系国際標準の状態 |
| [ANSI ISO 1989:1985 catalog](https://webstore.ansi.org/standards/iso/iso19891985) | official | 1985 版が ANSI X3.23-1985 を採用したこと、amendment |
| [ANSI INCITS 23-1985 catalog](https://webstore.ansi.org/standards/incits/ansiincits231985r2001) | official | ANSI COBOL-85 の目的と対象 |
| [INCITS 2002 版発表](https://www.incits.org/news-events/press-releases/incits-approves-revised-isoiec-cobol-standard-as-an-american-national-standard) | official | 2002 版の主要拡張 |
| [INCITS 2023 版発表](https://www.incits.org/news-events/press-releases/available-now-2023-edition-of-isoiec-1989-cobol) | official | 2023 版の主要変更 |
| [NIST CCVS85 記録](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication305supp25.pdf) | official | COBOL 85 Compiler Validation System の存在と位置付け |
| [日立 COBOL マニュアル一覧](https://itpfdoc.hitachi.co.jp/Pages/document_list/manuals/cobol.html) | reference | COBOL2002 製品マニュアル、拡張仕様、ユーザーズガイド |
