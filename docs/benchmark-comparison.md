# COBOL コンパイラ/実行系 ベンチマーク比較

## N-Queens ベンチマーク（n=1..13）

最も信頼できる比較データ。GO TO ループの性能が支配的。

| 実行系 | 時間 | C比 |
|---|---|---|
| IBM Enterprise COBOL (z/OS, OPTIMIZE FULL) | 0.37s | 0.9x |
| GnuCOBOL 4.0 (-O2) | ~3.0s | 1.2x |
| GnuCOBOL 3.2 (-O2, 最適化コーディング) | 1.14s | — |
| GnuCOBOL 3.2 (-O2, 標準) | 9.78s | — |
| gcobol (GCC COBOL frontend 13.0) | 309.8s | 738x |
| C (GCC -O2) 参考値 | 0.42s | 1.0x |

GnuCOBOL 4.0 が C の 1.2 倍まで改善したのが最大の競合。IBM Enterprise COBOL はメインフレーム専用なので直接競合しない。

## 主要 COBOL 実行系一覧

| 実行系 | 方式 | プラットフォーム | ライセンス |
|---|---|---|---|
| IBM Enterprise COBOL | ネイティブ (z/OS) | メインフレーム | 商用 |
| Micro Focus Visual COBOL | ネイティブ/JVM/.NET | x86/クラウド | 商用 |
| GnuCOBOL | C 変換→gcc/clang | Linux/macOS/Win | GPL |
| gcobol | GCC フロントエンド | Linux | GPL |
| Cobalt | LLVM (Rust 製) | Linux/macOS | MIT |
| rust-cobol | C 変換→clang | Linux/macOS | MIT/Apache |

## 浮動小数点ベンチマーク（Fourmilab）

| 実行系 | C比（倍率） |
|---|---|
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
