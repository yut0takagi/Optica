# Optica — 言語仕様 対応表 と 未対応構文の明示エラー化（Issue #3）

- 日付: 2026-07-12
- ブランチ: `fix/spec-table-and-unsupported-errors`
- 対象 Issue: #3「言語仕様と実装済み機能の対応表を整備し、未対応構文を明示的にエラーにする」

## スコープ

Issue #3 の完了条件のうち、本ブランチでは以下 3 点を扱う（ブランチ名に一致）:

1. **対応表**: 構文カテゴリごとの `supported / partial / planned / unsupported` 表を新規ドキュメントに整備。
2. **未対応構文のエラー化**: 現状「黙ってスキップ」される未対応構文を、既定でエラー・`--allow-unsupported` で警告スキップに変える。
3. **回帰テスト**: 代表的な未対応構文の既定エラー / フラグ許可 / 未定義関数呼び出し / 既存正当モデルの回帰。

**スコープ外（別 PR）**: examples ファイルの「実行可能 / 将来仕様」ディレクトリ分割（Issue #3 完了条件②）。20 本超のファイル移動を伴い差分が大きいため分離する。本 PR では対応表で各 example の状態を示すに留める。

## 現状の問題（コード根拠）

- `src/parser.rs` の文ディスパッチ ([parser.rs:182-196](../../../src/parser.rs)) で、`def ` / `bellman ` / `transition:` / `terminal ` / `initial:` は空行・コメントと同じ分岐で **黙って `continue`（スキップ）** される。ユーザーはモデル化ミス（未対応構文の使用）に気づけない。
- 目的関数・制約内の `total_cost(i)` のようなユーザー定義関数呼び出しは、組み込み関数（`min max abs sqrt exp log pow`）以外なので式パーサの「素のシンボル」分岐に落ち、`total_cost` が未知シンボル検査で拾われうるが、メッセージが `unknown symbol` で関数呼び出しとして不親切、かつ後続の `(...)` の扱いが曖昧。
- 一方、既に Fase1 で **未対応演算子・未知 set 名・subject to 外の `forall`** は明示エラー化済み（main 側）。本件はその「サイレントスキップ撲滅」路線の続き。

`stage ` / `state ` / `decision ` は `parse_stage` / `parse_state_or_decision` で **実際にパースされ set/var が登録される**（DP の遷移・ベルマン再帰の意味論は未評価）。したがってこれらは「**partial**」であり、本件ではエラー化せず対応表で「partial（DP 意味論は未評価）」と明記するに留める。

## 設計

### 全体方針: Issue #6（`--allow-missing-params`）パターンの踏襲

一貫性のため、既存の未設定パラメータ診断と同じ構造にする:

- `parse()` の署名は変えない。パース中に未対応構文を検出したら **`Model` の診断フィールドに記録してスキップ継続**する。
- `main.rs` がパース後にそのフィールドを見て、フラグにより **エラー終了 or 警告継続** を決める。

### B1. 未対応構文の検出（`src/parser.rs`）

- `Model` に診断フィールドを追加:
  ```rust
  /// 未対応（planned）構文を検出した行の説明。既定ではエラー、--allow-unsupported で警告。
  pub unsupported: Vec<String>,
  ```
  `Model::new()` で空 `Vec` 初期化。
- 判定ヘルパを追加:
  ```rust
  /// 行が「未対応（planned）」構文で始まるなら人間可読な構文名を返す。
  fn unsupported_construct(line: &str) -> Option<&'static str> {
      const TABLE: &[(&str, &str)] = &[
          ("def ",        "def (user-defined functions)"),
          ("bellman ",    "bellman (dynamic-programming recursion)"),
          ("transition:", "transition (DP state transition)"),
          ("terminal ",   "terminal (DP terminal condition)"),
          ("initial:",    "initial (DP initial condition)"),
      ];
      TABLE.iter().find(|(p, _)| line.starts_with(p)).map(|(_, name)| *name)
  }
  ```
- ディスパッチループの先頭（空行・コメント・`model `/`problem `/`end`/`}` の無害スキップ判定より **前**）で:
  ```rust
  if let Some(name) = unsupported_construct(line) {
      model.unsupported.push(name.to_string());
      continue; // 従来通りスキップ。方針判断は main 側。
  }
  ```
  無害スキップ集合からは `def ` / `bellman ` / `transition:` / `terminal ` / `initial:` を除去する（重複判定を避ける）。`model ` / `problem ` / `end` / `}` は従来通り無害スキップに残す。

### B2. 未定義関数呼び出しの明示エラー（`src/expr.rs`）

式パーサの識別子処理 `_ =>` 分岐（[expr.rs:351](../../../src/expr.rs)、組み込み関数にマッチしなかった識別子）で、直後のトークンが `(`（`Tok::LPar`）の場合は **関数呼び出し構文** と判定して明示エラーにする:

```
unknown function '<name>': user-defined functions are not supported (see docs/SPEC_SUPPORT.md)
```

これにより `total_cost(i)` は「関数として未対応」という正確なメッセージで即エラーになり、`(...)` 引数リストの曖昧な解釈を排除する。添字 `[...]` を伴う既存の正当なシンボル参照（`x[i]`）は `(` ではなく `[` なので影響しない。

### B3. CLI フラグ（`src/cli.rs` / `src/main.rs`）

- `cli.rs`: `Args` に `pub allow_unsupported: bool` を追加。全 `Args { .. }` 構築箇所（`parse()` の 2 箇所、`main.rs` の REPL 用構築）で初期化。オプション解析ループに `"--allow-unsupported" => allow_unsupported = true,` を追加。`--allow-missing-params` と同じく `start_idx` 経路に載るのでファイル直指定形式でも有効（#4 の恩恵）。
- `main.rs`: `missing_params` 診断ブロックの直後（同じ流儀）で:
  ```rust
  if !model.unsupported.is_empty() {
      if args.allow_unsupported {
          eprintln!("warning: unsupported constructs skipped: {}", model.unsupported.join(", "));
      } else {
          eprintln!("error: unsupported constructs used: {}", model.unsupported.join(", "));
          eprintln!("hint: these are planned but not yet implemented (see docs/SPEC_SUPPORT.md); \
                     pass --allow-unsupported to skip them");
          std::process::exit(1);
      }
  }
  ```
  配置は `dim == 0` チェックより **前** に置く（`def` 等を含むモデルは変数が少なく `no variables` が先に出て未対応メッセージが埋もれるのを防ぐ）。

### 順序に関する注記

`def` と、その関数を参照する目的関数（`total_cost(i)`）の両方を含むモデルでは、`parse()` 内で B2 の関数呼び出しエラーが先に返り、`main.rs` の未対応診断に到達しないことがある。どちらも「ユーザー定義関数は未対応」という同一根本原因を指すため許容する。テストは構文を分離したケースで各挙動を検証する。

## 対応表ドキュメント（A1）

- 新規: `docs/SPEC_SUPPORT.md`。
- 列: `構文 / 状態 (supported|partial|planned|unsupported) / 備考`。
- カテゴリ: 変数・パラメータ・集合宣言、目的関数、制約（演算子・`forall`・`sum`）、式（組み込み関数・`if/then/else`）、多目的（weighted_sum / epsilon）、DP（`stage`/`state`/`decision`=partial、`bellman`/`transition`/`terminal`/`initial`=planned）、ユーザー定義関数 `def`=planned、確率計画・CP グローバル・`import`=planned/unsupported。
- 各 example ファイルの現状（実行可能 / EXPERIMENTAL）も表または箇条書きで併記。
- README の既存注記（未対応構文の散文）から本ドキュメントへリンクを 1 本張る。

## テスト（C）: `tests/unsupported_constructs.rs`

`tests/param_diagnostics.rs` と同じ `run()`（実バイナリを一時ファイルに対して実行）ハーネスを流用する:

1. `def_errors_by_default` — `def total_cost(i) -> real: ...` を含むモデルは既定で失敗し、stderr に `def` と `docs/SPEC_SUPPORT.md` を含む。
2. `unsupported_allowed_with_flag` — `--allow-unsupported`（`solve` サブコマンド形式）で成功し、stderr に `warning` と構文名を含む。
3. `bellman_transition_terminal_initial_error_by_default` — 各構文が既定でエラー（パラメータ化 or 個別テスト）。
4. `unknown_function_call_errors` — `def` 無しで目的関数に `total_cost(i)` を書くと `unknown function` を含むエラー。
5. `valid_model_still_parses` — 既存の正当モデル（例: `f1_lp_production` 相当のインライン LP）が引き続き成功する回帰。

加えて `src/parser.rs` 内の `#[cfg(test)]` に `unsupported_construct()` の単体テスト（各プレフィックス → Some、正当行 → None）を 1 本追加。

## 影響範囲・互換性

- 変更ファイル: `src/parser.rs`、`src/expr.rs`、`src/cli.rs`、`src/main.rs`、新規 `docs/SPEC_SUPPORT.md`、新規 `tests/unsupported_constructs.rs`、`README.md`（リンク 1 行）。
- 破壊的変更: `def`/`bellman`/`transition:`/`terminal`/`initial:` を含む既存 `[EXPERIMENTAL]` example は **既定でエラー**になる（従来は誤った結果を返していたので妥当）。`--allow-unsupported` で従来のスキップ挙動に戻せる。
- `parse()` の公開署名は不変（呼び出し側・REPL に影響なし）。

## 検証

- `. "$HOME/.cargo/env" && cargo test`（全既存テスト + 新規テストが緑）。
- 実 CLI で手動確認: `def` を含むファイルが既定でエラー / `--allow-unsupported` で警告継続 / 正当モデルが従来通り解ける。
