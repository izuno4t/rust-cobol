# NIST CM CI 失敗原因と解消記録

## 現在の状態

GitHub Actions 上では未解消。

2026-05-07 時点のローカル現行ワークツリーでは、Linux x86_64 コンテナ内の
release build で CM の直接対象とモジュール全体が PASS することを確認した。

```text
make nist-run MODULE=CM PROGRAM=CM102M NIST_JOBS=1
  CM102M: PASS

make nist-run MODULE=CM PROGRAM=CM202M NIST_JOBS=1
  CM202M: PASS

make nist-run MODULE=CM NIST_JOBS=1
  CM: Total 9, Pass 9, Fail 0, Ready 0, CErr 0, RErr 0, Rate 100%
```

一方で、GitHub Actions run
<https://github.com/izuno4t/rust-cobol/actions/runs/25498348522/job/74824357745>
は commit `87421961fd629e141d3943abc99dde23ca1cecd7` を checkout しており、
この commit では CM 用通信 fixture が存在しないため、`COBOL_COMM_SCRIPT` が
実行時に有効にならず CM が 3/9 PASS のまま失敗している。

この文書は、失敗時の原因分析、ローカル解消確認、CI 未反映原因を残す記録として
扱う。

## 背景

GitHub Actions run
<https://github.com/izuno4t/rust-cobol/actions/runs/25495909892> と
<https://github.com/izuno4t/rust-cobol/actions/runs/25498348522/job/74824357745>
で `nist-cm` ジョブまたは `nist-module (CM)` ジョブが失敗した。

対象 run の `Rust` workflow は `build` ジョブには成功している。失敗箇所は
`nist-cm` ジョブの `Verify NIST CCVS 85 CM pass rate` ステップである。

## CI 上の直接原因

`make nist-run MODULE=CM` 相当の実行結果が、CM モジュール 9件中 3件 PASS、
6件 FAIL だった。

```text
CM: Total 9, Pass 3, Fail 6, Ready 0, CErr 0, RErr 0, Rate 33%
```

CI の検証ステップは CM モジュールの全件 PASS を要求しているため、この結果で
ジョブが失敗した。

さらに、run `25498348522` の artifact と対象 commit を確認した結果、
CI では `tests/nist/fixtures/comm/CM102M.comm` と
`tests/nist/fixtures/comm/CM202M.comm` が checkout された commit に存在しない。
`tests/nist/run_nist.sh` は通信 fixture ファイルが存在する場合だけ
`COBOL_COMM_SCRIPT` を export し、存在しない場合は unset する。

したがって CI では、CM の通信キュー、destination、key、route/enqueue を
シミュレートする fixture が実行時に効いていない。

## 失敗したプログラム

- `CM101M`
- `CM102M`
- `CM103M`
- `CM104M`
- `CM105M`
- `CM202M`

## 失敗理由の分類

6件の失敗は同一原因ではない。大きく次の2種類に分けられる。

- 明示的な CCVS FAIL が出ているもの
  - `CM102M`
  - `CM202M`
- レポートは出力されているが、CCVS サマリとして判定できないもの
  - `CM101M`
  - `CM103M`
  - `CM104M`
  - `CM105M`

## 個別原因

### CM102M

`DISABLE`、`ENABLE`、`SEND` の異常系ステータス検証で失敗している。

NIST ソースでは `CM-OUTQUE-1` に次の通信 CD が定義されている。

```cobol
CD  CM-OUTQUE-1 FOR OUTPUT
    DESTINATION COUNT IS ONE
    TEXT LENGTH IS MSG-LENGTH
    STATUS KEY IS STATUS-KEY
    ERROR KEY IS ERR-KEY
    SYMBOLIC DESTINATION IS SYM-DEST.
```

期待される主な戻り値は次の通り。

| 条件 | 期待ステータス |
| --- | --- |
| 宛先未指定 | `20` |
| 不正パスワード | `40` |
| 宛先数 0 | `30` |
| 無効な宛先 | `20` |
| 送信文字数過大 | `50` |

実際には複数箇所で `00/0` が返っている。これは、`DESTINATION COUNT`、
`TEXT LENGTH`、`STATUS KEY`、`ERROR KEY`、`SYMBOLIC DESTINATION` が parser、
HIR lowering、codegen、runtime のいずれかで期待通り接続されていない可能性が高い。

### CM202M

`RECEIVE` が `QUEUE TESTED EMPTY` になっている。これは、送信済みメッセージが
受信側 `CM-INQUE-1` に届いていない、または受信 selector が一致せず空キュー扱いに
なっていることを示す。

また、terminal の `ENABLE` / `DISABLE` 異常系でも不一致が出ている。

| 条件 | 期待ステータス |
| --- | --- |
| 不正パスワード | `40` |
| 不正 source name | `21` |
| destination count が table 上限を超過 | `30` |
| 2件目の symbolic destination が不正 | `20` と `ERR-KEY(2)=1` |

`CM202M` では次の出力 CD が使われる。

```cobol
CD  CM-OUTQUE-1 OUTPUT
    DESTINATION COUNT DEST-COUNT
    TEXT LENGTH OUT-LENGTH
    STATUS KEY OUT-STATUS
    DESTINATION TABLE OCCURS 2 TIMES INDEXED BY I1
    ERROR KEY ERR-KEY
    DESTINATION SYM-DEST.
```

したがって、`SYM-DEST OCCURS 2`、`ERR-KEY OCCURS 2`、`DEST-COUNT`、stride、
area length が `cobol_comm_send` に正しく渡る必要がある。

### CM101M

実行ログは出ているが、CCVS の通常サマリまで到達していない。artifact の reason は
次の通り。

```text
undecidable-ccvs-output|nonempty-report-without-summary
```

初期 `ENABLE` までは進んでいるが、その後の通信入力、メッセージ件数、終了条件の
いずれかが期待通り進まず、判定器が PASS と断定できていない。

### CM103M

`MESSAGE LOG` は出ているが、`SEND STAT 60`、`MSG LENGTH 000` の行が大量に出ている。
これはメッセージ送信が空メッセージ扱いになっていることを示す。

`SEND FROM` の実データ長、`TEXT LENGTH`、または送信レコードの storage 解決が
期待と合っていない可能性が高い。

### CM104M

`MESSAGE LOG` のヘッダは出ているが、実メッセージ行がない。

複数キュー、複数宛先、または受信ルートの fixture が成立しておらず、判定器が期待する
通信ログが生成されていない。

### CM105M

CCVS ヘッダと列ヘッダだけで、テスト結果行とサマリがない。

QUEUE SERIES 系の通信入力が進まず、テスト本体が完走できていない状態と考えられる。

## 根本原因の仮説

当初の中心的な仮説は、`COMMUNICATION SECTION` サポートが NIST CM 全体を満たす
段階に達していないことだった。

特に次の要素が不足または不整合を起こしている可能性が高い。

- output CD の `DESTINATION COUNT`
- output CD の `SYMBOLIC DESTINATION`
- output CD の `DESTINATION TABLE OCCURS`
- output CD の `ERROR KEY`
- input/output CD の `STATUS KEY`
- input/output CD の `TEXT LENGTH`
- terminal source name と key の検証
- SEND された message の route/enqueue
- RECEIVE selector と queue name の照合

## 解消方針

CI 閾値を下げるのではなく、明示的な FAIL が出ていた `CM102M` と `CM202M` から
修正する方針を採った。

`CM101M`、`CM103M`、`CM104M`、`CM105M` は、通信基盤の不整合により完走または
判定ができていない副作用である可能性が高かった。先にこれらへ個別の判定器調整を
入れると、根本原因を隠すおそれがあった。

最新の CI 失敗については、実装そのものではなく、CI が checkout した commit に
通信 fixture ファイルが含まれていないことが直接原因である。必要な解消は、CM 用の
fixture ファイルを CI 対象 commit に含めることであり、CI の pass rate 条件を緩める
ことではない。

## 実施結果

- ローカル現行ワークツリーでは `CM102M` は PASS しており、通信異常系ステータスの
  不一致は再現しない。
- ローカル現行ワークツリーでは `CM202M` は PASS しており、`QUEUE TESTED EMPTY` と
  destination table/error key 系の不一致は再現しない。
- ローカル現行ワークツリーでは `CM101M`、`CM103M`、`CM104M`、`CM105M` も CM 全体
  実行で PASS している。
- GitHub Actions の対象 commit `87421961fd629e141d3943abc99dde23ca1cecd7` では、
  `tests/nist/fixtures/comm/CM102M.comm` と
  `tests/nist/fixtures/comm/CM202M.comm` が存在しないため、同じ結果にならない。
- `crates/cobol-driver/tests/e2e_test.rs` には、`DESTINATION TABLE OCCURS 2` と
  `ERROR KEY OCCURS 2` の full area length を codegen が渡すことを固定する
  E2E がある。
- `crates/cobol-runtime/src/communication.rs` には、destination count、invalid
  destination、source/key 検証、route/enqueue、receive selector の runtime
  処理がある。

## 実施済みチェックリスト

- [x] `CM102M` の生成 C または debug 出力で、`cobol_comm_disable`、
  `cobol_comm_enable`、`cobol_comm_send` に渡る引数を確認する
- [x] parser と HIR lowering で、`FOR OUTPUT` CD の `DESTINATION COUNT`、
  `TEXT LENGTH`、`STATUS KEY`、`ERROR KEY`、`SYMBOLIC DESTINATION` が保持される
  ことを固定する
- [x] codegen または runtime のテストで、`CM102M` 相当の異常系が `20/1`、
  `40/0`、`30/0` を返すことを固定する
- [x] `CM202M` 用に destination table 2件と error key 2件の layout を固定する
  テストを追加する
- [x] `DEST-COUNT = 3` で `30`、2件目の宛先不正で `20` と `ERR-KEY(2)=1` が返る
  ことを runtime/codegen テストで固定する
- [x] terminal `ENABLE` / `DISABLE` の key/source 検証を確認し、不正 password が
  `40`、不正 source が `21` になるようにする
- [x] `CM-OUTQUE-1` から `CM-INQUE-1` への route/enqueue と `RECEIVE` selector を
  確認し、`QUEUE TESTED EMPTY` を解消する
- [x] `CM102M` と `CM202M` の改善後、残り4件のログを再評価する

## 検証順序と結果

直接対象を狭く確認してから、CM 全体に広げる。

```sh
make nist-prepare
make nist-run MODULE=CM PROGRAM=CM102M
make nist-run MODULE=CM PROGRAM=CM202M
make nist-run MODULE=CM
make nist-summary
```

今回の確認結果は次の通り。

| コマンド | 結果 |
| --- | --- |
| `make nist-prepare` | PASS |
| `make nist-run MODULE=CM PROGRAM=CM102M NIST_JOBS=1` | `CM102M: PASS` |
| `make nist-run MODULE=CM PROGRAM=CM202M NIST_JOBS=1` | `CM202M: PASS` |
| `make nist-run MODULE=CM NIST_JOBS=1` | `9/9 PASS` |
| `make nist-summary` | `CM 9/9 PASS` |

実装修正後は、リポジトリ規約に従って最終的に次も実行する。

```sh
make clean test lint
```

## 判断記録

検討した選択肢は次の通り。

| 選択肢 | 評価 |
| --- | --- |
| CI の CM pass rate 条件を緩める | 短期的に CI は通せるが、通信実装の回帰検出を弱める |
| 判定器で通信系の一部を PASS 扱いにする | 症状を隠す可能性が高く、根本原因の特定に不向き |
| `CM102M` と `CM202M` の明示 FAIL から直す | 原因を観測しやすく、他4件の副作用も解消する可能性がある |

現時点では、`CM102M` と `CM202M` の明示 FAIL から直す方針が、正しさと効率の
両面で優位である。
