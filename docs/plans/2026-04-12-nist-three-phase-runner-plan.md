# NIST Three-Phase Runner Plan

Date: 2026-04-12

## Goal

NIST テストスイートを、テスト単位で独立に並列実行できる 3 フェーズ実行基盤へ組み替える。

フェーズは以下の 3 つに固定する。

1. compile
2. execute
3. collect

## Requirements

- NIST テストスイートを一括で実行できること
- compile / execute / collect の 3 フェーズで処理を分離すること
- 各テストを独立した単位として扱い、compile / execute の並列実行が可能であること
- compile フェーズは全件成功時のみ execute フェーズへ進むこと
- execute フェーズはテストごとにサブプロセスを起動し、全体の上限並列数 `NIST_JOBS` 内で並列実行すること
- collect フェーズは各テスト結果ディレクトリを参照して結果を集計すること
- 標準出力では、モジュール名と進捗件数が追えること
- 失敗時は各テストのログを結果ディレクトリに残し、追跡可能であること

## Problems in the Previous Runner

- 並列単位が実質モジュール寄りで、テスト単位並列になっていなかった
- compile / execute / collect の責務が `run_program` とフェーズ制御で混線していた
- 全体実行中にコンパイラ実体の参照が不安定で、偽の `COMPILE_ERROR` を大量発生させていた
- 進捗表示が逐次ログ寄りで、どのモジュールが何件進んだか追いづらかった

## New Design

## Phase 1: compile

- 対象モジュール配下の全テストから `module|program` のタスクリストを作る
- 親プロセスがタスクを最大 `NIST_JOBS` 件ずつ投入する
- 各タスクは `run_nist.sh __compile_one <MODULE> <PROGRAM>` を子プロセスとして起動する
- 子プロセスは `run_program module program compile_only` を実行し、前処理とコンパイルだけを行う
- 親プロセスは完了ごとに `[compile] MODULE done/total total_done/total PROGRAM STATUS` を表示する
- 1 件でも `COMPILE_ERROR` があれば execute フェーズへ進まない

## Phase 2: execute

- compile 成功後、同じタスクリストを execute 用として利用する
- 各タスクは `run_nist.sh __execute_one <MODULE> <PROGRAM>` を子プロセスとして起動する
- 子プロセスは `run_program module program run_only` を実行し、テスト準備・テスト実行・結果検証を完結させる
- 各テストは独立した作業ディレクトリと結果ディレクトリを使う
- 親プロセスは完了ごとに `[execute] MODULE done/total total_done/total PROGRAM STATUS` を表示する

## Phase 3: collect

- `results/<MODULE>/*.status` と関連ログを集計する
- モジュール単位 summary と全体 summary を出力する
- collect は並列化せず、結果ディレクトリを唯一の真実として扱う

## Isolation Rules

- 各テストの workdir は `ENV_ROOT/work/run/<MODULE>/<PROGRAM>/`
- 各テストの結果は `ENV_ROOT/results/<MODULE>/`
- コンパイラ本体は実行開始前に `ENV_ROOT/toolchain/` に固定コピーし、全ワーカーはそのパスだけを参照する
- 実行時エイリアス、入力 fixture 解決、出力 P ファイル準備、判定は execute 子プロセス内で完結させる

## Progress Output Contract

標準出力はフェーズ単位の見出しと、テスト単位進捗を出す。

例:

```text
=== Phase: compile ===
[compile] NC 17/95 total 44/391 NC123A COMPILED
=== Phase: execute ===
[execute] NC 12/95 total 201/391 NC123A PASS
=== Phase: collect ===
```

## Error Handling

- 各テストの compile log は `results/<MODULE>/<PROGRAM>.compile.log`
- 各テストの runtime log は `results/<MODULE>/<PROGRAM>.log`
- 失敗理由は `results/<MODULE>/<PROGRAM>.reason`
- 失敗時も collect フェーズで結果を回収できるよう、ログと status を必ず残す

## Implementation Status

- `run_nist.sh` を 3 フェーズ親オーケストレーションへ切り替え済み
- `__compile_one` / `__execute_one` の子プロセス実行を追加済み
- モジュール件数つき進捗表示を追加済み
- コンパイラスナップショットを `ENV_ROOT/toolchain/` に固定する処理を追加済み
- 今後は collect の全件検証と、並列実行時の残留競合を継続検証する
