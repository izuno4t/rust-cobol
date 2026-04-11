# Runtime ABI Contract

## 目的

この文書は、`cobol-codegen` が生成する C と `cobol-runtime` の間で共有する
ABI 契約を固定するためのものである。

Phase 3 の時点では、少なくとも次を明示する。

- runtime が担う COBOL 実行意味
- codegen が担う物理配置と呼び出し組み立て
- C ABI 上で安定して扱う型と関数群
- まだ opaque/raw のまま残っている契約

## 責務分担

### codegen が担うこと

- 解決済み HIR を C へ落とす
- data item の物理配置、offset、stride、record 長を決定する
- paragraph/section/label の解決済み遷移を C の制御構文へ写像する
- runtime 呼び出しに必要な descriptor や長さ情報を組み立てる

### runtime が担うこと

- COBOL の文字列 MOVE/STRING/UNSTRING/INSPECT の実行意味
- decimal 算術と表示形式変換
- file I/O、sort/merge、clock、threading、exception/call stack の実行意味
- codegen が渡した descriptor と長さ情報に従った実処理

### codegen が担ってはいけないこと

- 名前解決の再実行
- paragraph/label の再探索
- runtime 関数が期待するレイアウトの推測
- 匿名 struct や `void*` 前提の偶然一致に依存した ABI 接続

## 安定 ABI 型

現在、generated C から named type として使う ABI 型は次の 4 つである。

### `CobolDecimal`

- 用途: fixed-point decimal の runtime 表現
- 所有者: runtime
- 生成側責務: decimal data item と temporary をこの型で保持する
- 実行側責務: scale 調整、演算、表示変換、桁あふれ時の切り詰め

フィールド:

- `value`: スケーリング前提の整数値
- `scale`: 小数桁数
- `size`: PICTURE 上の総桁数
- `is_signed`: signed decimal かどうか

### `CobolStringSource`

- 用途: `STRING` 文の source descriptor
- 所有者: codegen が配列を組み立て、runtime が読む
- 意味:
  - `ptr` / `len`: source field
  - `delim_ptr` / `delim_len`: `DELIMITED BY` 指定

### `CobolUnstringTarget`

- 用途: `UNSTRING` 文の target descriptor
- 所有者: codegen が配列を組み立て、runtime が書き込む
- 意味:
  - `ptr` / `len`: target field
  - `delimiter_ptr` / `delimiter_len`: matched delimiter の返却先
  - `count_ptr`: moved character count の返却先

### `SortKey`

- 用途: `SORT` / `MERGE` の key descriptor
- 所有者: codegen が offset/length/order/type を決定し、runtime が比較に使う
- 意味:
  - `offset`: record 先頭からの byte offset
  - `length`: key byte length
  - `ascending`: 昇順かどうか
  - `key_type`: comparison category

`key_type` は generated C では次の定数を使う。

- `SORT_KEY_ALPHA`
- `SORT_KEY_SIGNED_BINARY`
- `SORT_KEY_UNSIGNED_BINARY`
- `SORT_KEY_DISPLAY_NUMERIC`

## 呼び出し契約

### CALL / GOBACK / exception

- `cobol_call_enter` と `cobol_exception_push` は `uintptr_t` として
  `jmp_buf` のアドレスを受け取る
- codegen は `setjmp` の生存期間内でのみそのアドレスを渡す
- runtime はそのアドレスを保存するが、所有しない

### STRING / UNSTRING

- codegen は named descriptor 配列を生成して runtime に渡す
- runtime は descriptor を読み、COBOL の区切り/桁送り/space padding を実行する
- descriptor の寿命は call 中のみで十分とする

### SORT / MERGE

- record buffer の物理 layout は codegen が決定する
- runtime は `SortKey` と `record_len` を使って比較する
- decimal field を sort 用に binary 化するかどうかは codegen が決める

### decimal

- decimal field のメモリ表現は `CobolDecimal` で固定する
- runtime は `CobolDecimal` 間演算を提供する
- codegen は ad-hoc な decimal 演算式を直接生成しない

## まだ raw/opaque のまま残している契約

次は Phase 3 で文書化対象に含めるが、descriptor 型まではまだ整理していない。

- communication runtime
  - destination / error-key / area-count などを長い生引数列で受けている
  - 物理 layout の情報自体は渡せているが、named descriptor には未整理
- JSON / XML runtime
  - runtime 側には `#[repr(C)]` field descriptor がある
  - ただし現行 codegen 側の lowering と C 組み立ては opaque な呼び出しのまま
  - 将来的には HIR 側で field descriptor を明示化し、typed ABI に寄せる

## 実装上の単一ソース

generated C に埋め込む runtime 宣言は
`crates/cobol-runtime/src/abi.rs` の `emit_c_declarations` を単一ソースとする。

これにより:

- codegen 側で `extern` 宣言を重複管理しない
- named ABI 型の追加や変更を 1 箇所で追跡できる
- ABI 変更のレビュー対象を明確化できる

## 変更時のルール

- ABI 型の field 追加・削除・並び変更は互換性変更として扱う
- `emit_c_declarations` を変更するときは、対応する runtime 実装と generated C
  利用箇所を同時に更新する
- 新しい runtime API を追加する場合は、まず named descriptor で表現できるかを検討する
- `void*` や匿名 struct を新規導入しない
