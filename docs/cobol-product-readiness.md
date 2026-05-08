# COBOL 製品化判断基準

更新日: 2026-05-08

この文書は、`rust-cobol` を製品レベルとして扱えるかを判断するための基準を
定義する。仕様適合テストに通ることは必要条件だが、十分条件ではない。

## 結論

- 製品化判断は、仕様適合、実運用耐性、互換性、診断性、保守性、
  リリース運用の 6 軸で行う。
- NIST CCVS85、E2E、unit test、lint が通っても、未管理の
  `Partial` / `Experimental` 領域がある場合は製品化可とは判断しない。
- `PASS` 率だけでなく、失敗時の診断、障害復旧、OS 差、データ互換性、
  ABI 互換性を gate として扱う。
- 未達項目は [製品レベルまでの残課題一覧](./production-gaps.md) に残し、
  完了条件と検証結果が揃うまで完了扱いにしない。

## 製品化判断と適合性検証の違い

| 観点 | 適合性検証 | 製品化判断 |
| --- | --- | --- |
| 主な問い | 仕様どおりに動くか | 実運用で安全に使えるか |
| 主な根拠 | unit test、E2E、NIST、比較検証 | 適合性、耐久性、互換性、診断性、運用性 |
| 成功条件 | 対象仕様の PASS | release gate 全体の通過 |
| 失敗の扱い | 機能単位で FAIL / 未検証 | 製品化ブロッカーまたは既知制約 |
| 対象 | 言語機能 | 言語機能、runtime、toolchain、OS、運用 |

## Release gate

製品化可と判断するには、すべての gate を満たす。

| Gate | 必須条件 | 主な検証 |
| --- | --- | --- |
| G1: 仕様適合 | 対象標準モードの仕様領域が `Verified` または明示的な既知制約で管理されている | `cargo test`、E2E、NIST |
| G2: 実行時耐性 | 長時間実行、大量データ、異常系、境界値で破綻しない | stress test、benchmark、fault injection |
| G3: データ互換性 | file format、numeric storage、encoding、ABI の互換性が文書化されている | snapshot、cross-version test |
| G4: 診断性 | compile/runtime error が原因追跡可能な形で報告される | diagnostic test、failure artifact review |
| G5: OS 互換性 | macOS、Linux、Windows または対象 OS の差分が管理されている | platform CI、runtime-x86、Windows docs |
| G6: リリース運用 | build、install、CI、rollback、既知制約の公開手順がある | release checklist、CI result、docs review |

## 判定状態

| 状態 | 意味 |
| --- | --- |
| `Ready` | すべての gate を通過し、未達制約が製品利用を妨げない |
| `Limited ready` | 用途や標準モードを限定すれば利用可能 |
| `Not ready` | 主要 gate に未達がある |
| `Unknown` | 判断に必要な検証結果がない |

`Unknown` は成功扱いにしない。`Limited ready` は対象範囲、非対象範囲、
回避策を必ず明記する。

## Gate 詳細

### G1 仕様適合

仕様適合は [COBOL 適合性検証計画](./cobol-conformance-verification.md) に従う。

必須条件:

- 対象標準モードが明確である
- 正例、負例、標準モード別の許可/拒否がある
- NIST 対象領域は module 単位で結果を確認している
- `INSPECT`、未検証、手動確認を `PASS` に含めない

### G2 実行時耐性

仕様上正しくても、実運用で破綻する場合は製品化可にしない。

確認対象:

- 大容量 sequential / indexed / relative file
- 大量 `CALL`、nested program、exception
- decimal 算術の境界値、overflow、丸め
- `SORT` / `MERGE` の大容量入力
- `JSON` / `XML` の不正入力、巨大入力、encoding
- 長時間実行時の memory、file descriptor、temporary file

### G3 データ互換性

COBOL はデータ互換性への影響が大きいため、次を gate とする。

- numeric display、packed decimal、binary storage の仕様化
- record layout、`REDEFINES`、`RENAMES`、`OCCURS` の ABI 互換
- file status、record locking、commit / rollback の対応範囲
- 生成 C と runtime ABI の互換性
- 既存バイナリまたは既存ファイルとの移行方針

### G4 診断性

失敗時に原因が追えることを製品化条件に含める。

必須条件:

- compile error に source span と原因がある
- runtime error に file status、exception code、対象操作が残る
- NIST / CI failure artifact を再現に使える
- unsupported feature は誤コンパイルせず、診断または明示的制約にする

### G5 OS 互換性

対象 OS を明示し、OS 差を未確認のまま製品化可にしない。

確認対象:

- macOS / Linux / Windows の path、newline、locale、process、signal
- C compiler と linker の差
- file locking と permission
- runtime archive の探索
- x86_64 Linux runtime と host 開発環境の差

### G6 リリース運用

製品化には、実装だけでなく出荷後の運用可能性が必要である。

必須条件:

- release build と install path が検証されている
- CI の失敗時にログと artifact を確認する手順がある
- 既知制約が user guide と production gaps に反映されている
- 破壊的変更の migration note がある
- rollback または revert の判断基準がある

## 現時点の判断

現時点では、全体を無条件に `Ready` と判断しない。

理由:

- `docs/production-gaps.md` に `Partial` / `Experimental` 領域の
  製品レベル未達が残っている。
- NIST CCVS85 は COBOL-85 系に有効だが、COBOL 2002 以降をカバーしない。
- runtime ABI、file I/O、exception、JSON/XML、communication などは
  実運用耐性と互換性の gate が残る。
- COBOL 2023 新規機能は仕様本文ベースの網羅検証が不足している。

したがって、現時点で妥当な表現は `Limited ready` または `Not ready` であり、
対象用途と対象標準モードを限定せずに製品化可とは言わない。

## 完了条件

製品化可を報告する前に、次を確認する。

- `docs/cobol-conformance-verification.md` の対象領域が `Verified` または
  明示的な既知制約になっている
- `docs/production-gaps.md` に P0/P1 の未解決ブロッカーがない
- `make clean test lint` が成功している
- 対象範囲の NIST module 結果が記録されている
- 対象 OS の CI または手元検証が成功している
- user-facing な既知制約が `docs/user-guide.md` に反映されている
