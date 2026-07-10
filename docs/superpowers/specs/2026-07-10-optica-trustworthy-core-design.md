# Optica Fase1 設計書：信頼できるコア（The Trustworthy Core）

- 日付: 2026-07-10
- ブランチ: `feature/phase1-trustworthy-core`
- ステータス: 設計承認済み（実装計画待ち）
- スコープ: Fase1 のみ（Fase2 は別スペックで扱う）

---

## 1. 背景と問題認識

現状の Optica はコード・同梱バイナリの両面でほとんど正しく動作していない。改善の入口はここにある。

現状診断（コード読解＋同梱バイナリの挙動検証で確認）:

1. **ビルドが通らない（致命的）**: `src/solver/mod.rs:17` と `src/solver/mod.rs:39` で
   `pub mod cpsat;` が二重宣言されており、Rust の E0428 コンパイルエラーになる。
   `cargo build` / `cargo install` は失敗し、CI（`cargo clippy -D warnings` / `cargo test`）も赤のはず。
2. **同梱バイナリも誤答**: リポジトリに追跡された `optica` バイナリ（現ソースからはビルド不能なため stale と判断）
   は自明な問題を解けない。
   - `minimize obj: 5`（定数）→ 0 を返す
   - `maximize obj: y`（y≤5）→ 0、1 反復で "optimal" と誤申告
   - README のナップサック例 → Objective 0、変数なし
3. **最大化が構造的に壊れている**: 収束判定 `if best_fit < TOLERANCE`
   （`mod.rs:101`, `mod.rs:272`、`TOLERANCE=1e-10`）は「最適値=0 の最小化」前提。
   最大化は内部で符号反転され fitness が負になるため、最初の反復で即座に収束扱いになり、
   初期値付近のゴミを返す。全最大化問題が破綻する。
4. **整数・バイナリ変数が効かない**: DE/PSO は連続 f64 のみ。`binary`/`int` は境界を設定するだけで
   整数性を強制しない（丸め・分枝限定なし）。ナップサック等は連続緩和になる。
5. **式パーサが簡易すぎる**: `parser.rs` の `eval_arith` 周辺は文字列ベースで、括弧・`sum`・`max/min`・
   複数行 `maximize name:` を取りこぼし、多くの正しい式で 0 を返す。しかも毎評価ごとに文字列を再解析している。
6. **テストが存在しない**: CI は `cargo test` を回す宣言だが `#[test]` も `tests/` もゼロ。回帰を防げない。

したがって高速化や機能追加の前に「まず正しく動く土台」が必要。

## 2. ゴール（Fase1 の定義）

Optica がクリーンにコンパイルでき、**DE/PSO＋整数性＋新 AST 評価器で厳密/検証可能に解ける全 example が、
正解（既知最適値 ± 許容誤差、かつ実行可能）を返す**こと。これをゴールデンテストで固定し、CI を本当に緑にする。
stale なバイナリを撤去する。

### 非ゴール（Fase2 送り）
- DP（後ろ向き DP / ステージ展開）、確率計画（シナリオ展開＝確定等価）、TSP/VRP（実行可能＋既知上界以下）、
  CP グローバル制約の厳密解（cp-sat）。これらは Fase2 の別スペックで扱う。
- 真の MILP（分枝限定）。整数性は「丸め込み修復」で近似する。

## 3. 例のフェーズ振り分け（棚卸し結果）

Fase1 対象（厳密最適を検証可能／今のアーキテクチャで解ける）:

| ファイル | 種別 | 備考 |
|---|---|---|
| `examples/01_lp_production.optica` | LP | 連続、厳密 |
| `examples/02_milp_facility.optica` | MILP(binary) | 丸め修復で解く（小規模前提） |
| `examples/03_nlp_portfolio.optica` | NLP(exp) | 新 AST の `exp` が必要 |
| `examples/04_convex_svm.optica` | convex/NLP | |
| `examples/05_qp_regression.optica` | QP/NLP | |
| `examples/11_moo_supply_chain.optica` | MOO(weighted/epsilon) | 既存 MOO 経路を利用 |
| `examples/12_ml_optimization.optica` | NLP(binary) | |
| `examples/knapsack.optica` | binary | README 掲載例 |
| `examples/simple_knapsack.optica` | binary | |
| `examples/advanced_features.optica` | 混在 | スモークのみ（パース成功＋実行可能） |

Fase2 送り（experimental 明示）:
`06_dp_inventory` `07_stochastic_farmer` `08_combinatorial_tsp` `09_metaheuristic_vrp`
`10_cp_scheduling` `13_largescale_decomposition` `juku_timetabling`

> 実装計画の段階で、各 Fase1 例が本当に厳密検証可能かを最終確認し、
> 想定より難しい例（例: `02_milp_facility` が大規模）は「実行可能＋上界」テストに格下げ、
> または Fase2 送りにする判断を明記する。

## 4. アーキテクチャと変更点

### 4.1 ビルド修復（前提）
- `src/solver/mod.rs` の重複 `pub mod cpsat;` を1つに統一し、`#[cfg(feature = "cp-sat")]` の
  ゲート・フォールバック（`solve_cp` の cfg 分岐）と整合させる。
- 受け入れ: `cargo build` と `cargo clippy --all-targets -- -D warnings` がデフォルト機能で成功。
  `--features cp-sat` はゲートのみ検証（OR-Tools 依存が無い環境ではビルド対象外として扱う）。

### 4.2 AST 式評価器（新規 `src/expr.rs`）
文字列の毎回再解析を廃止し、**パース時に式を 1 度だけ AST 化 → 何度も評価**する。

- 字句（Lexer）: 数値 / 識別子 / `[` `]`（添字）/ `(` `)` / `,` / `+ - * / ^` / 比較演算子（`< <= > >= == !=`）/ 関数名
- 文法（再帰下降 or Pratt）:
  - `primary := number | symbol index? | func '(' args ')' | '(' expr ')' | '-' primary`
  - `symbol index := ident ('[' idxlist ']')?`（`idxlist` は添字トークン、`env` で束縛値に解決）
  - `func := max | min | abs | sqrt | exp | log | pow`
  - `sum := ('sum' ('{' | '(') iters ('}' | ')')) body`（`iters := (ident 'in' set (',' ident 'in' set)*)`、`set` は集合名 or `a..b`）
  - `cond := expr (cmp expr)?`、`ifexpr := 'if' cond 'then' expr 'else' expr`
  - 二項演算子は優先順位: `^` > 単項`-` > `* /` > `+ -`
- AST 構造（例）:
  ```
  enum Expr {
    Num(f64),
    Var(usize),                 // 解決済み変数インデックス
    Param(String, Vec<IdxTok>), // param 参照（添字は env 解決）
    ParamScalar(String),
    Bin(Op, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Func(FuncKind, Vec<Expr>),
    Sum(Vec<(String, SetRef)>, Box<Expr>),
    If(Box<Cond>, Box<Expr>, Box<Expr>),
  }
  ```
- 評価は `(x: &[f64], env: &HashMap<String,String>, model: &Model) -> f64` を取る再帰関数。
  添字を含む `Var`/`Param` は `env` の束縛（例: `i -> "3"`）で最終キーに解決する。
- **効果**: 括弧・`sum`・定数・`max/min` が 0 を返すバグを一掃。かつ毎評価の文字列パース消滅で大幅高速化。

> 設計判断: 文（statement）レベルは既存の行ベース解析を残し、**式だけ** AST 化する。
> `Model` は目的/制約/`sum` の本体を `String` ではなくコンパイル済み `Expr` として保持する。

### 4.3 文パーサの堅牢化（`src/parser.rs`）
- 複数行の `maximize name:` / `minimize name:` / 制約の継続行を、`;` か次のキーワード
  （`subject to` / `var` / `param` / `set` / `maximize` / `minimize` / `objectives:` / `data:` など）
  まで連結してから式としてコンパイルする。→ 「例が読めない」問題を解消。
- 制約は `lhs OP rhs` の両辺を式として許可（現状の「RHS は数値かスカラーのみ」を撤廃）。
  違反量は `eval(lhs) - eval(rhs)` から算出。
- 変数ごとに整数フラグを保持する `Model.var_int: Vec<bool>` を追加（`binary`/`int`/`Integer` を捕捉）。
  `binary` は境界 `[0,1]` かつ整数。
- パース失敗は Result で明示エラーにする（4.5 参照）。

### 4.4 ソルバー正当性（`src/solver/mod.rs`）
- **最大化バグ修正**: `if best_fit < TOLERANCE { return }` の即時収束を撤去。
  正しい停止条件に置換する:
  - max_iter まで回す、または「改善が `STALL_ITERS` 反復連続で無ければ停止」。
  - **負の fitness を収束扱いにしない**。`TOLERANCE` は「相対/絶対の改善幅」判定にのみ使う。
  - → 全最大化問題が直る。
- **整数性の丸め込み修復（repair）**: `compute_fitness` の内部で、評価に用いる候補ベクトルの
  int/binary 次元を境界内で最近整数に丸めてから目的・制約を評価する。
  DE/PSO/hybrid のすべての評価経路、および最終報告解に一貫適用する。
  - 実装位置の候補: `compute_fitness(model, x)` の先頭で `x` を `repair(model, x)` した作業バッファに写してから評価。
    探索は連続空間で行い、評価と報告は整数化した値で行う（丸め込み法）。
- **目的値の正当表示**: 報告解 `best` を整数丸めしたうえで目的関数を再評価して表示。
  現状の「表示目的値が実解と食い違う」問題を解消する。

### 4.5 エラーハンドリング
- 未知シンボル / 未知関数 / 括弧不整合 / 不正な `sum` ヘッダは、**サイレント 0 ではなく明示的な
  パースエラー**（行番号＋理由）にする。「見かけ optimal で誤答」を根絶する。
- 評価時の数値例外は NaN を避ける: `log(x)` の `x<=0`、`/0`、`sqrt(負)`、`pow` のオーバーフローは
  大きな有限ペナルティ or クランプで扱う（メタヒューリスティック評価が壊れないため）。方針を実装計画で確定。

### 4.6 データフロー
```
source
  → 文パーサ（行ベース、継続行連結）
  → Model { vars(+var_int), params, sets, 目的Expr(s), 制約{lhs:Expr, op, rhs:Expr}, MOO設定 }
  → ソルバー DE/PSO/hybrid
       fitness = ±目的 + PENALTY·(制約違反 + CPペナルティ)   ※評価前に整数丸め repair
  → best 報告（整数丸め → 目的を再評価して表示）
```

## 5. テスト戦略（TDD）

各バグに「失敗するテストを先に」書いてから実装する。

### 5.1 ユニットテスト（`#[cfg(test)]`）
- 字句/AST 評価:
  - 定数: `5 → 5`、`2*3 → 6`、`10-2*3 → 4`、`(2+3)*2 → 10`、`2*(3+4) → 14`、`2^3 → 8`
  - 関数: `max(2,7) → 7`、`3+max(2,7) → 10`、`abs(-4) → 4`、`sqrt(9) → 3`
  - 変数/param 添字: `sum{i in S} p[i]*x[i]` が正しい値
  - `if a < b then x else y`
  - 単項マイナス・優先順位・ネスト括弧
- 丸め修復: binary/int 次元が整数に固定され、境界内に収まること
- 制約違反: `Le/Ge/Eq` の違反量が正しいこと（両辺が式のケース含む）
- パースエラー: 未知関数・括弧不整合が `Err` になること

### 5.2 統合テスト（`tests/examples.rs`）
- Fase1 各例について、既知最適値 ± 許容誤差かつ実行可能であることを検証する期待値表を同梱。
  - 例: `knapsack.optica` の profit 期待値、`01_lp_production` の total_profit 期待値 等。
  - 期待値は解析解 or 信頼できる別ソルバーで一度求めた値を「golden」として固定。
- `advanced_features.optica` はスモーク（パース成功＋実行可能解）。
- 決定性のため乱数シードは固定（既存の固定シードを踏襲）。テストは `-m de` 等メソッド固定で実行。

### 5.3 CI
- 既存の `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` が
  デフォルト機能で緑になること。

## 6. リポジトリ衛生
- 追跡中の `optica` バイナリを `git rm` し、`.gitignore` に `/optica` と `/target` を追加。
- README: パフォーマンス表・言語仕様のクレームを実態に合わせて更新。Fase2 例を「experimental」明示。
- 各 Fase2 例のヘッダにも experimental 注記。
- `CHANGELOG.md` に Fase1 の修正内容を追記。

## 7. 受け入れ基準（Definition of Done）
1. `cargo build` / `cargo clippy --all-targets -- -D warnings` / `cargo test` がデフォルト機能で成功。
2. Fase1 対象の全 example が、golden 期待値 ± 許容誤差かつ実行可能を返す（統合テストで検証）。
3. 最大化・括弧・定数・`sum`・`max/min` の各再現テストが緑。
4. 未知シンボル/関数/括弧不整合が明示エラーになる。
5. 追跡バイナリ撤去、README/CHANGELOG 更新済み。
6. Fase2 例が experimental として明示され、CI を壊さない（テスト対象外 or スモークのみ）。

## 8. リスクと制約
- **重要**: 開発に使うこの環境には Rust ツールチェーンが無く（`cargo` 不在）、
  設計者側でのコンパイル/テスト実行ができない。**実装とテスト実行はユーザー環境または CI で行う**。
  実装計画は「テストは書くが実行は cargo 環境で回す」前提で、検証ステップを CI/ローカルに委譲する。
- 丸め込み修復は真の MILP ではないため、`02_milp_facility` 等が大規模だと厳密最適に届かない可能性。
  → 計画段階で tractability を確認し、必要なら「実行可能＋上界」テストに格下げ、または Fase2 送り。
- AST 化に伴い `Model` の型が変わる（`String` → `Expr`）ため、`main.rs`/`solver` の呼び出し側修正が必要。

## 9. 未決事項（実装計画で確定）
- 数値例外（log/0除算/sqrt負）の具体的なペナルティ方針。
- `STALL_ITERS` の値と、既存 `TOLERANCE` 定数の再定義。
- `02_milp_facility` 等の golden 期待値の算出手段。
