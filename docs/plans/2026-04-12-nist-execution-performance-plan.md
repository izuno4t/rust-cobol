# NIST Execution Performance Improvement Plan

**Goal:** NIST CCVS 85 の壁時計時間を、正しさを落とさずに大幅短縮する

**Primary Outcome:** `make nist-run` の遅さを、生成コード単体の問題ではなく
ハーネスの共有状態と execute 直列化の問題として整理し、
各 program の独立性を強制したうえで全件を program 単位で並列実行できるようにする

**Scope:** `tests/nist/run_nist.sh` を中心に、program 単位の workdir / tmpdir 分離、
並列実行モデル、judge 連携、タイムアウト方針を整理する

**Non-Goal:** 個々の timeout ケースに対するその場しのぎの例外追加や、
mode を増やして一部だけ速く見せること

---

## 問題設定

現在の NIST 実行時間を支配しているのは、主として次の要因である。

- execute phase がほぼ直列である
- runtime の作業ディレクトリが module 単位で共有されている
- timeout が一律で長い
- `manual-report` / 対話待ち系を通常実行してから timeout 判定している
- program ごとの入出力 alias / report 出力の独立性がハーネス上で明示されていない

この結果、
「本当に重いプログラムが遅い」のではなく
「待たなくてよいケースを長く待っている」時間が大きくなっている。

したがって改善目的は、
**各 program が他 program とファイルシステム状態を共有しないようにし、
その前提で全件を同じ program-level queue に流せるようにする**
ことと定義する。

---

## 観測済みの根本原因

## 1. compile と execute の実行モデルが非対称

`run_nist.sh` は compile phase にだけ並列化の足場があり、
execute phase はモジュール直列・プログラム直列で実行されている。

結果として、compile error が解消した後も壁時計時間はほとんど改善しない。

## 2. timeout が一律 60 秒

`TIMEOUT_SECONDS=60` が全件に適用されるため、
`manual-report` や対話待ち系、fixture 待ち系が
「即失敗すべきケース」でも 60 秒消費しうる。

## 3. 実行前に分類できるケースを実行している

少なくとも以下は、通常実行レーンから切り離せる。

- `manual-report`
- `subprogram-only`
- `dummy-display`
- 一部 judge で compile log / runtime log だけから判定できるケース

## 4. workdir / tmpdir が program 単位で分離されていない

前処理では `NIST_TMPDIR` を program ごとに割り当てている一方で、
runtime 実行時は module 単位の workdir に `chdir` している。

このずれがあると、
relative file access や report 出力、fixture alias が
program 単位で閉じず、execute の全面 parallel 化を妨げる。

---

## 基本方針

## 方針 1: program 単位の実行環境を完全分離する

- binary
- preprocessed source
- tmpdir
- print file
- runtime current directory

を program ごとに閉じる。

## 方針 2: timeout をカテゴリ別にする

- 通常ケース: 短め
- `manual-report` / 対話待ち系: さらに短くする
- 既知の大型ケース: 専用上限を持たせる

## 方針 3: execute parallelism は module 単位ではなく program 単位

compile 完了後は module ごとの区切りを維持しつつも、
実行キュー自体は全 module 横断で program 単位に流す。

---

## 段階計画

## Phase 1: 独立実行基盤の導入

- source 由来の属性判定を関数化する
- timeout を program 属性で切り替えられるようにする
- program ごとの workdir / tmpdir / alias を固定する

## Phase 2: execute phase の並列化

- 全 module 横断で program job queue を作る
- log / status / result の分離を維持する

## Phase 3: 運用パラメータ整理

- `NIST_JOBS` / `NIST_EXEC_JOBS` の意味を execute 全体並列に合わせる
- CI とローカルで適切な既定値を検討する

---

## 今回着手する実装

今回の初回着手では、リスクが低く効果が大きい部分から入る。

1. source ベースの実行属性関数を導入する
2. `manual-report` 系 timeout を短縮する
3. program 単位の workdir / tmpdir / alias を完全分離する
4. execute phase を全 module 横断の program queue に変更する

保留:

- CI 既定値の最適化
- timeout 属性表の完全整備

---

## 成功条件

以下を満たしたとき、この改善は正しく進んでいるとみなす。

1. compile / execute の結果判定ロジックが壊れていない
2. 並列実行しても status / log / print file が衝突しない
3. `manual-report` による無駄な待ち時間が明確に減る
4. 通常ケースの壁時計時間が短縮する

---

## 検証方針

- 代表 module を対象に、変更前後で status 分布を比較する
- `parallel-safe` module で log / status の破損がないことを確認する
- `manual-report` 代表ケースで timeout 短縮が効くことを確認する

---

## 注意事項

- 全件一括 parallel 化はやらない
- 特殊系を通常系に混ぜない
- 速度改善のために判定正しさを弱めない
