# Optica Fase1 設計書：信頼できるコア（The Trustworthy Core）

- 日付: 2026-07-10
- ブランチ: `feature/phase1-trustworthy-core`
- ステータス: 実環境で診断を実証・訂正済み（再承認待ち）
- スコープ: Fase1 のみ（Fase2 は別スペック）

> 注意: 本スペックは初版で誤診（「ビルド不能」「最大化全壊」等）を含んでいた。
> Rust ツールチェーンを導入し、**新規ビルドしたバイナリ**で実測して診断を訂正した。
> 以下は訂正後の正確な内容である。

---

## 0. 実証環境
- rustup（stable, `rustc 1.97.0`, `cargo 1.97.0`, clippy/rustfmt 同梱）を導入済み。
- これにより設計者側でビルド/テスト/例実行が可能（初版にあった「cargo 不在」制約は解消）。

## 1. 背景と訂正済み現状診断

同梱の `optica`（Git 追跡）は **stale（古い壊れたビルド）** で、誤動作する。
初版はこの stale バイナリを叩いて誤診していた。新規ビルド（`cargo build --release`,
`target/release/optica`）で実測し直した結果が以下。

### ❌ 初版の誤り（撤回）
- 「ビルド不能（`pub mod cpsat;` 二重宣言で E0428）」→ **誤り**。現行 Rust で問題なくコンパイルし、
  `cargo clippy --all-targets -- -D warnings` も緑。重複 `mod` は同一モジュールに解決され、エラーにならない。
- 「CI が赤」→ **誤り**。fmt/clippy/test すべて緑（テスト0件のため自明に通過）。
- 「括弧・定数・優先順位・`sum` が 0 を返す」「最大化が全て 0」→ **誤り**。新規ビルドでは
  `(2+3)*2 → 10`、`max(2,7) → 7`、`if 1<2 then 10 else 20 → 10`、`maximize 3*y (y≤5) → 15` と動作する。

### ✅ 実在する本当の問題（新規ビルドで実証）
1. **複数行構文の取りこぼし（最重要／最大の影響）**
   `maximize name:` の次行に式本体を書くスタイル、および `subject to:` ブロックの複数行制約を、
   行ベースパーサが落とす。結果、目的式が空になり **ほぼ全 example が Objective≈0** を返す。
   これが「例が動かない」ことの支配的原因。
   - 実証: `maximize c:\n    sum(i in S) x[i]` → obj 0（本来 2 を最大化すべき）。
2. **早期収束バグ**
   `if best_fit < TOLERANCE`（`TOLERANCE=1e-10`, `mod.rs:101`/`mod.rs:272`）は「最適値=0 の最小化」前提。
   fitness が負（=負値の目的、全最大化は内部符号反転で負）になると **1 反復で即終了**する。
   - 実証: `maximize obj: 10-(y-3)*(y-3)`（内部最適 y=3）→ `Iterations=1`、y≈2.98 で停止。
     簡単な問題では DE 初回反復が偶然近い解を出すため「動いて見える」が、本質的に壊れており、
     難しい問題では早期打ち切りで質が落ちる。負値目的の最小化でも同様に発火する。
3. **数学関数の欠落・誤り**
   - `min(a,b)` が誤値を返す（実証: `min(2,7) → 7`）。
   - `abs / sqrt / exp / log` が **未実装**で、しかも**エラーにならず引数をそのまま返す**
     （実証: `abs(0-4) → -4`, `sqrt(9) → 9`, `exp(0) → 0`, `log(1) → 1`）。
   - `^`（べき乗）演算子が **未実装**（実証: `2^3 → 3`、`3^2 → 2`。演算子がトークナイズで落ち誤評価）。
   - NLP 系例（`03/04/05`）はこれらが無いと正しく評価できない。
4. **整数性の未強制**
   DE/PSO は連続 f64 のみ。`binary/int` は境界設定だけで整数性を強制しない（丸め・分枝限定なし）。
   実証: `simple_knapsack`（`var x[ITEMS] binary`）の解が x=0.33,0.47,… と連続値。
5. **stale な追跡バイナリ**
   追跡中 `./optica`（sha `807e…`）と新規ビルド（sha `fed0…`）はハッシュが異なり、前者は誤動作する。
6. **テストが存在しない**
   `#[test]` も `tests/` もゼロ。回帰を防げない。

## 2. ゴール（Fase1 の定義）

Optica が、**DE/PSO＋整数性＋（修正済み）式評価器で厳密/検証可能に解ける全 example について、
正解（既知最適値 ± 許容誤差、かつ実行可能）を返す**こと。これをゴールデンテストで固定し、
CI を「本当に意味のある緑」（テストあり）にする。stale バイナリを撤去する。

### 非ゴール（Fase2 送り）
- DP（bellman/stage/state 解釈）、確率計画（シナリオ展開＝確定等価）、TSP/VRP（実行可能＋既知上界以下）、
  CP グローバル制約の厳密解（cp-sat）。
- 真の MILP（分枝限定）。整数性は「丸め込み修復」で近似する。

## 3. 例のフェーズ振り分け

新規バイナリでの全例実測（ほぼ全て obj≈0＝複数行取りこぼし）を踏まえ、複数行パース修正後に
厳密検証可能となる見込みの例を Fase1、ソルバー拡張が要る例を Fase2 とする。

Fase1 対象（複数行パース＋関数＋整数性で厳密/検証可能を狙う）:
`01_lp_production` `02_milp_facility(binary)` `03_nlp_portfolio(exp)` `04_convex_svm`
`05_qp_regression` `11_moo_supply_chain(weighted/epsilon)` `12_ml_optimization(binary)`
`knapsack` `simple_knapsack`（＋`advanced_features` はスモーク）

Fase2 送り（experimental 明示）:
`06_dp_inventory` `07_stochastic_farmer` `08_combinatorial_tsp` `09_metaheuristic_vrp`
`10_cp_scheduling` `13_largescale_decomposition` `juku_timetabling`

> 実装計画で各 Fase1 例の tractability と golden 期待値を最終確認し、想定より難しい例は
> 「実行可能＋上界」テストに格下げ、または Fase2 送りにする判断を明記する。

## 4. アーキテクチャと変更点

### 4.1 複数行文パースの修正（最優先・`src/parser.rs`）
- `maximize name:` / `minimize name:` / `subject to:` 配下の制約について、`;` か次のトップレベル
  キーワード（`var`/`param`/`set`/`maximize`/`minimize`/`subject to`/`objectives:`/`data:` 等）
  が現れるまで**継続行を連結**してから式としてコンパイルする。
- ラベル行（`limit:` 等）＋次行に本体、というインデント構文も吸収する。
- 受け入れ: 複数行スタイルの例で目的・制約が正しく取り込まれること（obj が 0 でなくなる）。

### 4.2 AST 式評価器（新規 `src/expr.rs`）
文字列の毎回再解析を廃止し、**パース時に 1 度だけ AST 化 → 何度も評価**する。

- 字句: 数値 / 識別子 / `[]`添字 / `()` / `,` / `+ - * / ^` / 比較演算子 / 関数名
- 文法（再帰下降 or Pratt）: 単項`-`・括弧・優先順位（`^` > 単項`-` > `*/` > `+-`）、
  関数 `min max abs sqrt exp log pow`、`sum{i in SET,...} body` / `sum(...)`、`if cond then a else b`
- AST 例:
  ```
  enum Expr { Num(f64), Var(usize), Param(String, Vec<IdxTok>), ParamScalar(String),
              Bin(Op, Box<Expr>, Box<Expr>), Neg(Box<Expr>), Func(FuncKind, Vec<Expr>),
              Sum(Vec<(String, SetRef)>, Box<Expr>), If(Box<Cond>, Box<Expr>, Box<Expr>) }
  ```
- 評価: `(x, env, model) -> f64`。添字は `env` 束縛で最終キー解決。
- **未知関数・未知シンボル・括弧不整合はサイレント値ではなく明示エラー**にする（誤答の温床を断つ）。
- `Model` は目的/制約/`sum` 本体を `String` ではなくコンパイル済み `Expr` で保持する。
- 効果: `min` 誤り・`abs/sqrt/exp/log`/`^` 欠落を解消し、毎評価の文字列パース消滅で高速化。

### 4.3 ソルバー正当性（`src/solver/mod.rs`）
- **早期収束バグ修正**: `if best_fit < TOLERANCE { return }` を撤去。正しい停止条件へ置換:
  max_iter まで回す、または「改善が `STALL_ITERS` 反復連続で無ければ停止」。
  **負の fitness を収束扱いにしない**。`TOLERANCE` は改善幅判定のみに使う。DE(single/parallel)・PSO 双方。
- **整数性の丸め込み修復**: `compute_fitness` 内で、評価に使う候補の int/binary 次元を境界内で最近整数に
  丸めてから目的・制約を評価。DE/PSO/hybrid の全評価経路と最終報告解に一貫適用（探索は連続、評価/報告は整数）。
- **目的値の正当表示**: 報告解を整数丸めしたうえで目的を再評価して表示。

### 4.4 変数メタ情報
- `Model.var_int: Vec<bool>` を追加（`binary`/`int`/`Integer` を捕捉、`binary` は `[0,1]`＋整数）。

### 4.5 データフロー
```
source → 文パーサ（複数行連結） → Model{ vars(+var_int), params, sets, 目的Expr, 制約{lhs:Expr,op,rhs:Expr}, MOO }
       → ソルバー DE/PSO/hybrid（評価前に整数丸め; fitness=±目的+PENALTY·違反）
       → best 報告（整数丸め→目的再評価）
```

## 5. テスト戦略（TDD）

各 REAL バグに「失敗する回帰テストを先に」書いてから実装する。

### 5.1 ユニット（`#[cfg(test)]`）
- 式評価（既知の壊れケースを回帰化）:
  `min(2,7)→2`, `max(2,7)→7`, `abs(0-4)→4`, `sqrt(9)→3`, `exp(0)→1`, `log(1)→0`, `2^3→8`,
  `(2+3)*2→10`, `2*(3+4)→14`, 単項マイナス, ネスト括弧, `sum{i in S} p[i]*x[i]`, `if a<b then x else y`。
- 未知関数/シンボル/括弧不整合が `Err`。
- 複数行 `maximize name:` / `subject to:` ブロックの取り込み（目的/制約が非空）。
- 早期収束回帰: 内部最適・負値目的で 1 反復停止せず最適に収束すること（`maximize 10-(y-3)^2 → y≈3`）。
- 丸め修復: binary/int 次元が整数かつ境界内。
- 制約違反量（両辺が式のケース含む）。

### 5.2 統合（`tests/examples.rs`）
- Fase1 各例の目的値が既知最適 ± 許容誤差かつ実行可能であることを検証（期待値表を同梱）。
- `advanced_features` はスモーク（パース成功＋実行可能）。
- 決定性のため乱数シード固定・メソッド固定（`-m de`）。

### 5.3 CI
- `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` が緑（テストあり）。

## 6. リポジトリ衛生
- 追跡中 `optica` を `git rm` し、`.gitignore` に `/optica`・`/target` を追加。
- README: 言語仕様・制約の記述を実態に合わせて更新（複数行構文の対応、関数一覧、整数性）。
  Fase2 例を「experimental」明示。各 Fase2 例ヘッダにも注記。
- `CHANGELOG.md` に Fase1 の修正内容を追記。

## 7. 受け入れ基準（Definition of Done）
1. `cargo build` / `cargo clippy --all-targets -- -D warnings` / `cargo test` が緑（テストあり）。
2. Fase1 対象の全 example が golden 期待値 ± 許容誤差かつ実行可能を返す。
3. 回帰テスト（複数行パース・早期収束・`min/abs/sqrt/exp/log/^`・整数性）が緑。
4. 未知シンボル/関数/括弧不整合が明示エラー。
5. 追跡バイナリ撤去、README/CHANGELOG 更新済み、Fase2 例が experimental 明示。

## 8. リスクと制約
- 丸め込み修復は真の MILP ではないため、`02_milp_facility` 等が大規模だと厳密最適に届かない可能性
  → 計画段階で tractability を確認し、必要なら「実行可能＋上界」テストへ格下げ or Fase2 送り。
- AST 化で `Model` の型が変わる（`String`→`Expr`）ため、`main.rs`/`solver` 呼び出し側の修正が必要。
- 既存の MOO 経路（`compute_fitness` 内 weighted/epsilon）は式評価を新 AST 経由に差し替える。

## 9. 未決事項（実装計画で確定）
- 数値例外（`log(x≤0)`・0除算・`sqrt(負)`・`pow` オーバーフロー）のペナルティ/クランプ方針。
- `STALL_ITERS` の値と `TOLERANCE` 定数の再定義。
- 各 Fase1 例の golden 期待値の算出手段（解析解 or 信頼できる別計算）。
