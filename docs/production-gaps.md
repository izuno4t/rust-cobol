# 製品レベルまでの残課題一覧

作成日: 2026-03-01
現在のテスト数: 441（ユニット + E2E文字列マッチ中心）

---

## 1. 致命的：生成Cコードがコンパイルできない

### 1-1. GOTO ラベルが生成されない
- **場所**: `codegen.rs` — `HirStatement::GoTo` の処理
- **問題**: `goto label_PARA;` を生成するが、対応する `label_PARA:` ラベルを一切生成していない
- **影響**: GOTOを使うCOBOLプログラムは全てCコンパイルエラー
- **修正方針**: パラグラフ関数のエントリにラベルを生成するか、GOTOをパラグラフ関数呼び出しに変換

### 1-2. SORT USING のC関数呼び出しシグネチャ不一致
- **場所**: `codegen.rs` 1484-1504行付近
- **問題**: `cobol_file_open(handle, name, 1, 0)` と4引数で呼び出すが、ランタイムは7引数
- **影響**: SORT USING を使うプログラムはCコンパイルエラー
- **修正方針**: ランタイムのシグネチャに合わせてcodegenを修正

### 1-3. CORRESPONDING がフラット変数にドット記法を使う
- **場所**: `codegen.rs` — `emit_corresponding_move`, `emit_corresponding_arith`
- **問題**: `WS_SRC.FIELD_A` のようなC構造体メンバアクセスを生成するが、実際の変数はフラットな `static int64_t FIELD_A`
- **影響**: MOVE CORR / ADD CORR を使うプログラムはCコンパイルエラー
- **修正方針**: グループ項目の構造体化（1-4）と同時に修正するか、フラット変数名で展開する方式に変更

### 1-4. グループ項目がC構造体として生成されない
- **場所**: `codegen.rs` — `emit_data_items`, `emit_single_data_item`
- **問題**: グループ項目のメンバが個別のフラット `static` 変数として生成される。Cの `struct` にならない
- **影響**: グループ単位のMOVE、DISPLAY、比較、REDEFINESが全て不正動作。CORRESPONDINGも壊れる
- **修正方針**: `HirType::Group` を `typedef struct { ... } GROUP_NAME_t;` として生成。最も影響範囲が大きい根本修正

---

## 2. 高：コンパイルは通るが動作が間違う

### 2-1. MULTIPLY/DIVIDE GIVING のターゲットが無視される
- **場所**: `lower.rs` — `lower_multiply`, `lower_divide`
- **問題**: AST の `giving` フィールドが完全に無視される
- **影響**: `MULTIPLY A BY B GIVING C` が `B = A * B` になり、Cに結果が入らない
- **修正方針**: HIR に `giving` フィールドを追加し、codegen で結果を GIVING ターゲットに格納

### 2-2. INVALID KEY / ON OVERFLOW ハンドラが無条件実行
- **場所**: `codegen.rs` — WRITE, START, STRING, UNSTRING の各処理
- **問題**: `/* TODO: INVALID KEY check */` コメントだけで条件分岐なし。ハンドラの本体が常に実行される
- **影響**: ファイルI/Oのエラーハンドリングが壊れる（常にエラー扱い）
- **修正方針**: ランタイム戻り値を変数にキャプチャし、if文で条件分岐

### 2-3. ACCEPT FROM DATE/TIME/DAY が stdin から読む
- **場所**: `lower.rs` — `lower_accept`（`from` フィールド無視）、`codegen.rs` — Accept処理
- **問題**: `ACCEPT WS-DATE FROM DATE` が `fgets(stdin)` になる
- **影響**: 日付・時刻取得が全て壊れる
- **修正方針**: HIR に `AcceptSource` を追加、codegen でシステム日付取得関数を呼び出す

### 2-4. EXIT PARAGRAPH / EXIT SECTION が CONTINUE（no-op）になる
- **場所**: `lower.rs` 459行
- **問題**: パラグラフ/セクションの残りをスキップすべきだが、何もしない
- **影響**: 制御フローが壊れる
- **修正方針**: パラグラフ末尾へのgotoまたはreturnに変換

### 2-5. GOBACK がサブプログラムからプロセス終了する
- **場所**: `cobol-runtime/src/program.rs` — `cobol_goback()`
- **問題**: `std::process::exit(0)` を呼ぶ。CALLされたサブプログラムからは呼び出し元に戻るべき
- **影響**: サブプログラム構造を持つプログラムが壊れる
- **修正方針**: `setjmp/longjmp` または関数returnで呼び出し元に復帰

### 2-6. ALL figurative constant がゼロに変換される
- **場所**: `lower.rs` 323行
- **問題**: `ALL "X"` が `HirLiteral::Zero` にマップされる
- **影響**: `MOVE ALL "X" TO WS-FIELD` がゼロ移動になる
- **修正方針**: `HirLiteral::AllChar(char)` を追加し、codegen で `memset` 生成

### 2-7. ファイル組織が常に行順（org=1）
- **場所**: `codegen.rs` — Open文の生成
- **問題**: SELECT句のファイル組織（INDEXED/RELATIVE/SEQUENTIAL）がcodegenに渡されず、常に `org=1`
- **影響**: INDEXED/RELATIVEファイルのI/Oが全て壊れる（ランタイム実装はある）
- **修正方針**: HIRにファイル組織情報を追加し、codegen で正しい org 値を渡す

### 2-8. CALL ON EXCEPTION が発火しない
- **場所**: `codegen.rs` — `_call_failed` が常に0
- **問題**: 動的リンク失敗を検知していない
- **修正方針**: `dlopen/dlsym` 失敗時にフラグを立てる

---

## 3. 中：機能未実装

### 3-1. DECLARATIVES のパース未実装
- **場所**: `cobol-parser/src/proc_div.rs` 34行 — `let declaratives = Vec::new();`
- **状況**: HIR/lower/codegen は実装済みだがパーサが常に空Vecを返す
- **修正方針**: `DECLARATIVES ... END DECLARATIVES` ブロックのパースを実装

### 3-2. EVALUATE ALSO（複数サブジェクト）未対応
- **場所**: `cobol-parser/src/proc_div.rs` — `parse_when_object()`
- **問題**: WHEN句のALSOループがなく、単一オブジェクトのみ
- **修正方針**: ALSO区切りで複数オブジェクトをパースし、ネストIF生成を拡張

### 3-3. SORT INPUT/OUTPUT PROCEDURE が無視される
- **場所**: `lower.rs` 1184-1191行
- **問題**: `SortInput::InputProcedure` が `Vec::new()` に変換される
- **修正方針**: プロシージャ名を保持し、codegen でプロシージャ呼び出しを生成

### 3-4. PERFORM THRU のセクション横断
- **場所**: `codegen.rs` — emit_perform
- **問題**: パラグラフインデックスの解決が同一セクション内のみ
- **修正方針**: セクション+パラグラフの順序リストを構築し、範囲実行

### 3-5. SET ADDRESS OF — ポインタセマンティクス欠如
- **場所**: `lower.rs` 1105-1112行
- **問題**: アドレス操作が単純値代入に変換される
- **修正方針**: HIRにポインタ操作ノードを追加

### 3-6. ALLOCATE CHARACTERS 形式 — サイズ計算が間違う
- **場所**: `lower.rs` 1418行
- **問題**: 文字数ではなく `sizeof(_ALLOC_CHARS)` が使われる
- **修正方針**: 文字数を保持してcodegenで `malloc(n)` を生成

### 3-7. 組み込み関数の文字列戻り値未対応
- **場所**: `lower.rs` 1514行付近
- **問題**: TRIM, UPPER-CASE, LOWER-CASE 等が数値式として扱われる
- **修正方針**: 戻り型に応じたコード生成の分岐

### 3-8. SCREEN / REPORT / COMMUNICATION SECTION
- **場所**: パーサは一部パース、HIR lowerで完全無視
- **修正方針**: 優先度低。段階的に実装

### 3-9. XML PARSE が空コメントのみ
- **場所**: `codegen.rs` — XmlParse
- **修正方針**: ランタイムにXMLパーサ統合（libxml2等）

### 3-10. RAISE 例外が常にabort
- **場所**: `cobol-runtime/src/exception.rs`
- **修正方針**: setjmp/longjmpベースの例外伝播

### 3-11. INVOKE（OOPメソッド呼び出し）がスタブ
- **場所**: `cobol-runtime/src/exception.rs` 200-205行
- **修正方針**: vtableディスパッチの実装

### 3-12. DISPLAY of subscripted alphanumeric — 整数として表示
- **場所**: `codegen.rs` 1984-1994行
- **修正方針**: 型情報に基づいてcobol_display_str/cobol_display_intを使い分け

---

## 4. 今後の進め方（推奨）

### 原則
- **テストは必ずclangコンパイル→実行→出力検証のE2Eで書く**
- 文字列パターンマッチのユニットテストに頼らない
- 1つ修正するごとに `make example` 相当の実行確認

### 優先順位
1. **グループ項目の構造体化**（1-4）— 最も影響範囲が大きい根本修正
2. **GOTOラベル生成**（1-1）— 制御フローの基盤
3. **GIVING対応**（2-1）— 算術の基本機能
4. **ACCEPT FROM DATE/TIME**（2-3）— 頻出パターン
5. **INVALID KEY / ON OVERFLOW の条件分岐**（2-2）
6. **ファイル組織の正しいorg値渡し**（2-7）
7. **DECLARATIVES パース**（3-1）
8. 残りの中優先度タスク

### 検証用COBOLプログラム
実装の進捗確認用に、以下のパターンをカバーするテストプログラムを用意すべき：
- ファイル処理（OPEN/READ/WRITE/CLOSE + FILE STATUS + INVALID KEY）
- 算術（ADD/SUBTRACT/MULTIPLY/DIVIDE + GIVING + ON SIZE ERROR）
- 文字列操作（STRING/UNSTRING/INSPECT）
- 制御フロー（PERFORM THRU, GO TO, EVALUATE ALSO）
- サブプログラム（CALL + GOBACK）
- グループ項目（MOVE CORR, グループMOVE, REDEFINES）
