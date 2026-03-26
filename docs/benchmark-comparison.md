# COBOL コンパイラ/実行系 ベンチマーク比較

## 調査ベースの比較

既存の公開情報と記事をもとにした比較メモ。計測環境が異なるため、末尾のローカル実測結果とは分けて扱う。

## N-Queens ベンチマーク（n=1..13）

最も信頼できる比較データ。GO TO ループの性能が支配的。

| 実行系 | 時間 | C比 |
| --- | --- | --- |
| IBM Enterprise COBOL (z/OS, OPTIMIZE FULL) | 0.37s | 0.9x |
| GnuCOBOL 4.0 (-O2) | ~3.0s | 1.2x |
| GnuCOBOL 3.2 (-O2, 最適化コーディング) | 1.14s | --- |
| GnuCOBOL 3.2 (-O2, 標準) | 9.78s | --- |
| gcobol (GCC COBOL frontend 13.0) | 309.8s | 738x |
| C (GCC -O2) 参考値 | 0.42s | 1.0x |

GnuCOBOL 4.0 が C の 1.2 倍まで改善したのが最大の競合。IBM Enterprise COBOL はメインフレーム専用なので直接競合しない。

## 主要 COBOL 実行系一覧

| 実行系 | 方式 | プラットフォーム | ライセンス |
| --- | --- | --- | --- |
| IBM Enterprise COBOL | ネイティブ (z/OS) | メインフレーム | 商用 |
| Micro Focus Visual COBOL | ネイティブ/JVM/.NET | x86/クラウド | 商用 |
| GnuCOBOL | C 変換→gcc/clang | Linux/macOS/Win | GPL |
| gcobol | GCC フロントエンド | Linux | GPL |
| Cobalt | LLVM (Rust 製) | Linux/macOS | MIT |
| rust-cobol | C 変換→clang | Linux/macOS | MIT/Apache |

## 浮動小数点ベンチマーク（Fourmilab）

| 実行系 | C比（倍率） |
| --- | --- |
| C | 1.0x |
| Micro Focus Visual COBOL (COMP-2) | 12.5x |
| COBOL 固定小数点 | 46.3x |

## 参考: Cobalt (Rust 製 COBOL コンパイラ)

- GitHub: c272/cobalt
- GnuCOBOL との比較ベンチマークスイート内蔵
- cobc と clang を PATH に置いて比較実行可能
- 結果は JSON 形式で出力
- 同じ Rust+LLVM 系で最も近い比較対象

## 推奨ベンチマーク戦略

1. **hyperfine** で GnuCOBOL との E2E 比較（コンパイル+実行）
2. **criterion.rs** で cobol-runtime の BCD 算術マイクロベンチ
3. **N-Queens** を共通ベンチマークとして実装
4. **NIST CCVS 85 の実行時間**を GnuCOBOL と比較

## ローカル実測結果

## 計測環境

- 計測日: 2026-03-27
- OS: macOS 26.3.1 (arm64)
- C コンパイラ: Apple clang 17.0.0
- Rust: cargo 1.93.1
- 比較対象:
  `rust-cobol` (`cargo run --release --package cobol-driver --`) /
  `GnuCOBOL` (`cobc`)

## ローカル N-Queens ベンチマーク

`benchmarks/nqueens.cob` を `n=1..13` で実行し、`hyperfine --warmup 1 --runs 3` で比較しました。

| 実行系 | 平均実行時間 | 最小 | 最大 | 相対比 |
| --- | ---: | ---: | ---: | ---: |
| rust-cobol | 704.5 ms | 699.5 ms | 707.2 ms | 1.00x |
| GnuCOBOL | 20,806.0 ms | 20,781.7 ms | 20,845.6 ms | 29.54x |

`N-Queens` では `rust-cobol` が `GnuCOBOL` より約 `29.5x` 高速でした。

単発実行時の壁時計時間とコンパイル時間も以下の通りです。

| 実行系 | コンパイル時間 | 単発実行時間 |
| --- | ---: | ---: |
| rust-cobol | 432 ms | 0.837 s |
| GnuCOBOL | 221 ms | 21.137 s |

## ローカル マイクロベンチマーク

`tests/benchmark/run_bench.sh --compare gnucobol` を使い、算術・文字列操作・ファイル I/O を比較しました。

| ベンチマーク | rust-cobol 実行時間 | GnuCOBOL 実行時間 | 傾向 |
| --- | ---: | ---: | --- |
| arithmetic | 0.490 s | 0.372 s | rust-cobol は約 1.32x 遅い |
| string_ops | 0.383 s | 0.343 s | rust-cobol は約 1.12x 遅い |
| fileio | 0.291 s | 0.318 s | rust-cobol は約 1.09x 速い |

コンパイル時間は次の通りです。

| ベンチマーク | rust-cobol コンパイル | GnuCOBOL コンパイル |
| --- | ---: | ---: |
| arithmetic | 0.433 s | 0.146 s |
| string_ops | 0.218 s | 0.194 s |
| fileio | 0.222 s | 0.187 s |

## ローカル実測のまとめ

- 探索系の重いループを含む `N-Queens` では、`rust-cobol` は `GnuCOBOL` に対して大幅に高速だった
- 小規模なマイクロベンチマークでは、両者はおおむね同程度で、ワークロードにより優劣が入れ替わる
- 現状の実測からは、`rust-cobol` の実行系は「常に一様に速い」というより、CPU 集約の処理で大きく伸びる傾向がある

## 実行コマンド

```bash
bash benchmarks/run_benchmark.sh
bash tests/benchmark/run_bench.sh --compare gnucobol
```
