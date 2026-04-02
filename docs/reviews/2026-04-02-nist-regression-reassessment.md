# NIST 回帰対応の再評価

作成日: 2026-04-02
対象: `rust-cobol` ワークスペース全体
観点: NIST CCVS 85 回帰対応の進め方、変換契約、層ごとの責務境界

## 背景

NIST CCVS 85 の不具合対応は複数回実施しているが、個別プログラムを起点に
局所修正を積み増した結果、別モジュールの `COMPILE_ERROR` や挙動退行を誘発
している。

2026-04-02 時点の回帰結果では、`FAIL` や `RErr` ではなく `CErr` が広域に
発生している。

- `IC`: 2
- `IX`: 4
- `RL`: 3
- `SQ`: 28
- `TOTAL`: 37

この分布は、単一機能の不足ではなく、前処理から codegen までの共通基盤が
壊れていることを示している。

## 結論

従来の進め方は不適切だった。

- NIST の失敗をプログラム単位で追い、症状ごとに対処していた
- 変換段階ごとの保証を定義しないまま修正していた
- 同じ概念を複数層で再解釈する実装を許していた
- COBOL 上の論理表現と C/runtime 上の物理表現の対応を固定していなかった

今後は「NIST を通すための修正」ではなく、
「コンパイラが各段階で何を保証するかを再定義し、その契約に合わせて
再実装する」方針に切り替える。

## 何が不足していたか

前回までの「見直し」は、失敗プログラムの整理や原因候補の列挙に留まって
おり、設計監査としては不十分だった。

不足していたのは次の 3 点である。

### 1. 変換契約の明文化

各段階が何を受け取り、何を保証するかが曖昧だった。

- `fixed source -> preprocessed source`
- `preprocessed source -> tokens`
- `tokens -> AST`
- `AST -> HIR`
- `HIR -> C`
- `C -> runtime ABI`

この契約が曖昧なまま個別修正を行うと、ある層で吸収すべき問題を別の層で
補修する実装になり、再度の退行を招く。

### 2. 責務重複の禁止

同じ概念を複数層で再解釈していた。

特に危険なのは次の領域である。

- fixed-format continuation
- COPY/REPLACE 後の字句境界
- OCCURS を持つ data item のサイズと stride
- communication description の destination/error-key 表現

これらは 1 層に集約すべき処理が、preprocessor、lexer、parser、codegen、
runtime に分散している。

### 3. 物理表現の固定

COBOL の論理モデルと C/runtime の物理モデルが一致していない。

例として `PIC X OCCURS` は COBOL 上では要素列だが、C 側では
NUL 終端込みの配列として生成される場合があり、runtime がその stride を
理解していないと、別要素ではなく終端バイトを読み書きしてしまう。

これは単なる bug ではなく、表現モデル未定義の結果である。

## 現在の主要な破綻点

現状で優先的に再監査すべき破綻点は次の 4 つである。

### 1. fixed-format 正規化

fixed-format の物理行から論理行への変換が一箇所に閉じていない。

- continuation の結合規則
- quote-heavy literal の終端判定
- COPY/REPLACE 後の行再構成

これらが複数段階に分散している限り、`NC` や `SM` のような個別修正が
別モジュールの退行を招く。

### 2. preprocessor と lexer の境界

preprocessor が source text の意味変換だけでなく、字句境界や source format
の再解釈まで担っている箇所がある。

本来は次の形に寄せるべきである。

- preprocessor: COPY/REPLACE と固定形式の論理ソース正規化
- lexer: 正規化済みソースの tokenization

この境界が曖昧だと、`COMPILE_ERROR` の原因がどの層にあるか追跡できない。

### 3. OCCURS と配列表現

`OCCURS` を持つ item について、以下の値が混同されている。

- 論理要素数
- 要素サイズ
- C 上の stride
- area 全体サイズ

この混同は file I/O、communication、MOVE、subscripted access のすべてに
影響する。`SQ` や `IX` の広域 `CErr` はこの系統が疑わしい。

### 4. communication ABI

communication runtime の API は、logical item length と physical stride を
区別できていない。

たとえば destination table や error key は、runtime へ少なくとも次の情報を
渡せなければならない。

- base pointer
- logical item length
- physical stride
- element count
- total area length

現状の API はこの区別が弱く、codegen と runtime の責務分離も不十分である。

## 今後の修正原則

今後は次の原則を守る。

### 1. NIST 個別修正を先にしない

まず層ごとの最小再現テストを追加する。

- fixed-format
- preprocessor/lexer 境界
- parser/HIR 保持
- codegen/runtime ABI

NIST の個別プログラムは、その後の統合確認として使う。

### 2. 1 つの概念は 1 層で解釈する

例:

- continuation は fixed-format normalizer だけが解釈する
- COPY/REPLACE は preprocessor だけが解釈する
- OCCURS の物理配置は codegen が定義し、runtime は ABI を使う

### 3. 論理値と物理値を名前で分ける

サイズ系は少なくとも次を分離する。

- `item_len`
- `stride`
- `count`
- `area_len`

この区別が API と helper に現れない限り、再発を防げない。

### 4. 再実装は下層から行う

順序は固定する。

1. fixed-format normalizer
2. preprocessor/lexer boundary
3. data item size/stride model
4. communication/file/table runtime ABI
5. その後に NIST 再投入

## 当面の作業手順

次に行うべき作業は以下である。

1. `CErr 37` の発生プログラムを収集し、どの段階で落ちるかを分類する
2. `fixed-format`、`OCCURS`、`communication` を中心に最小再現を追加する
3. 各層の保証を壊している helper と API を洗い出す
4. 契約を定義したうえで下層から再実装する
5. その後に NIST 全体を再実行する

## 付記

今回の再評価で重要なのは、「前回も見直した」こと自体ではなく、
「見直しの粒度が設計監査として足りていなかった」点である。

今後は、症状の説明ではなく、変換契約と表現モデルを基準に修正方針を決める。
