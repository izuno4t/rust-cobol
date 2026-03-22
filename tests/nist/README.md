# NIST CCVS 85 Conformance Testing

COBOL-85 Compiler Validation System (CCVS 85) によるコンパイラ適合性検証。

## セットアップ

### 1. newcob.val の入手

GnuCOBOL SourceForge から `newcob.val` をダウンロード:

```bash
# GnuCOBOL SourceForge の nist ディレクトリから取得
curl -L -o newcob.val.Z \
  "https://sourceforge.net/projects/gnucobol/files/nist/newcob.val.Z/download"
uncompress newcob.val.Z
# または gzip 版
# gunzip newcob.val.gz
```

### 2. テストプログラムの抽出

```bash
perl extract.pl newcob.val programs/
```

抽出結果:

```text
programs/
  NC/    — Nucleus (核文法)
  SM/    — Source Manipulation (COPY文)
  IC/    — Inter-program Communication (CALL文)
  SQ/    — Sequential I/O
  IF/    — Intrinsic Functions
  IX/    — Indexed I/O
  RL/    — Relative I/O
  ST/    — SORT/MERGE
  RW/    — Report Writer
  DB/    — Debugging
  SG/    — Segmentation
  OB/    — Obsolete Features
```

### 3. テスト実行

```bash
# 単一モジュール実行
./run_nist.sh NC

# 単一プログラム実行
./run_nist.sh NC NC101A

# 全モジュール実行
./run_nist.sh --all

# 結果サマリー表示
./run_nist.sh --summary
```

各実行の終了時に、モジュール別の失敗一覧や単一プログラムの結果要約も標準出力へ表示される。

## 目標通過率

| モジュール | 目標 | 備考 |
| --- | --- | --- |
| NC (Nucleus) | 95%+ | 最優先。COBOL核文法の網羅テスト |
| IF (Intrinsic Functions) | 95%+ | 組み込み関数テスト |
| SQ (Sequential I/O) | 95%+ | 順編成ファイルI/O |
| IC (Inter-program Comm) | 90%+ | CALL文テスト |
| SM (Source Manipulation) | 80%+ | COPY文テスト |

参考: GnuCOBOL v2.2 は全体で 99.79% (9,688/9,708) を通過。

## 結果の読み方

`results/` ディレクトリに各プログラムの実行結果が保存される:

- `*.status` — PASS / FAIL / COMPILE_ERROR / RUNTIME_ERROR / TIMEOUT
- `*.log` — プログラムの標準出力
- `*.compile.log` — コンパイルエラー出力
- `summary.txt` — モジュール別の集計

## 環境変数

- `COBOLC` — コンパイラのパス (デフォルト: `cargo run --release --package cobol-driver --`)
