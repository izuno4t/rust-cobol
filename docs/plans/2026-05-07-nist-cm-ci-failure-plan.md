# NIST CM CI 失敗原因と修正方針

## 背景

GitHub Actions run
<https://github.com/izuno4t/rust-cobol/actions/runs/25495909892> で
`nist-cm` ジョブが失敗した。

対象 run の `Rust` workflow は `build` ジョブには成功している。失敗箇所は
`nist-cm` ジョブの `Verify NIST CCVS 85 CM pass rate` ステップである。

## 直接原因

`make nist-run MODULE=CM` 相当の実行結果が、CM モジュール 9件中 3件 PASS、
6件 FAIL だった。

```text
CM: Total 9, Pass 3, Fail 6, Ready 0, CErr 0, RErr 0, Rate 33%
```

CI の検証ステップは CM モジュールの全件 PASS を要求しているため、この結果で
ジョブが失敗した。

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

中心的な原因は、`COMMUNICATION SECTION` サポートが NIST CM 全体を満たす段階に
達していないことである。

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

## 修正方針

CI 閾値を下げるのではなく、明示的な FAIL が出ている `CM102M` と `CM202M` から
修正する。

`CM101M`、`CM103M`、`CM104M`、`CM105M` は、通信基盤の不整合により完走または
判定ができていない副作用である可能性が高い。先にこれらへ個別の判定器調整を入れると、
根本原因を隠すおそれがある。

## 実装順序

1. `CM102M` の生成 C または debug 出力で、`cobol_comm_disable`、
   `cobol_comm_enable`、`cobol_comm_send` に渡る引数を確認する。
2. parser と HIR lowering のテストを追加し、`FOR OUTPUT` CD の
   `DESTINATION COUNT`、`TEXT LENGTH`、`STATUS KEY`、`ERROR KEY`、
   `SYMBOLIC DESTINATION` が HIR に残ることを固定する。
3. codegen または runtime のテストを追加し、`CM102M` 相当の異常系が
   `20/1`、`40/0`、`30/0` を返すことを固定する。
4. `CM202M` 用に destination table 2件と error key 2件の layout を固定する
   テストを追加する。
5. `DEST-COUNT = 3` で `30`、2件目の宛先不正で `20` と `ERR-KEY(2)=1` が返る
   ことを runtime/codegen テストで固定する。
6. terminal `ENABLE` / `DISABLE` の key/source 検証を確認し、不正 password が
   `40`、不正 source が `21` になるようにする。
7. `CM-OUTQUE-1` から `CM-INQUE-1` への route/enqueue と `RECEIVE` selector を
   確認し、`QUEUE TESTED EMPTY` を解消する。
8. `CM102M` と `CM202M` が改善した後、残り4件のログを再評価する。

## 検証順序

直接対象を狭く確認してから、CM 全体に広げる。

```sh
make nist-prepare
make nist-run MODULE=CM PROGRAM=CM102M
make nist-run MODULE=CM PROGRAM=CM202M
make nist-run MODULE=CM
make nist-summary
```

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
