# Optica 言語仕様 対応表（Spec Support Matrix）

このドキュメントは、Optica DSL の各構文が現行実装でどこまで扱えるかを示します（Issue #3）。

## 状態の凡例

| 状態 | 意味 |
|------|------|
| **supported** | 実装済み。正しく解釈・評価される。 |
| **partial** | 一部のみ。パース/登録はされるが、専用の意味論は未評価。 |
| **planned** | 未実装。既定でパースエラー。`--allow-unsupported` で警告に留めてスキップ可能。 |
| **unsupported** | 未実装かつ当面予定なし。 |

`planned` 構文（`def` / `bellman` / `transition:` / `terminal` / `initial:`）を含むモデルは、**既定でエラー終了**します。従来これらは黙って読み飛ばされ、無制約・0 評価の誤った「最適値」を返し得たため、明示エラーに変更しました。従来のスキップ挙動が必要な場合は `--allow-unsupported` を渡してください（警告を出して続行）。

```bash
optica model.optica                       # planned 構文があれば error 終了
optica model.optica --allow-unsupported   # warning を出してスキップし続行
```

## 宣言

| 構文 | 状態 | 備考 |
|------|------|------|
| `var x >= lo <= hi;` / `var x[S] ...` | supported | 連続変数・添字付き変数 |
| `binary` / `int` 修飾 | supported | 整数性は丸め込み修復による**近似**（真の MILP ではない） |
| `param name real;`（値なしスカラー） | supported | 既知シンボルとして登録。サイドカー JSON 等で補完（#5） |
| `param p[S] real;` + データ | supported | 宣言のみで値が無いまま参照すると既定でエラー（#6, `--allow-missing-params` で 0 許可） |
| `set S = {A, B, ...};` / `set S = 1..N;` | supported | 列挙・範囲 |
| `set C = A * B [* ...];`（直積） | supported | タプル要素は `"a,b"` 連結。`x[c]` は `x[a,b]` と一致。オペランドは定義済み集合（#11） |
| `data:` ブロック / サイドカー `model.json` | supported | 外部データは JSON のみ |

## 目的関数

| 構文 | 状態 | 備考 |
|------|------|------|
| `maximize name: expr;` / `minimize name: expr;` | supported | 単一目的。複数行ブロックも可 |
| `objectives:` 多目的ブロック | supported | 下記の Pareto 手法で結合 |
| `pareto method: "weighted_sum"` + `weight` | supported | 重み付き和（ヒューリスティック評価） |
| `pareto method: "epsilon_constraint"` + `primary`/`epsilon` | supported | ε 制約法（ヒューリスティック評価） |

## 制約

| 構文 | 状態 | 備考 |
|------|------|------|
| `subject to name: expr OP rhs;`（`<=` `>=` `=`） | supported | インライン・ブロック両形式 |
| `forall i in S: ...`（subject to 内） | supported | 添字展開。subject to 外の `forall` はエラー |
| 上記以外の制約演算子 | planned/error | 未対応演算子は明示エラー（Fase1 で対応済み） |
| 未知の集合名・シンボル | error | `unknown symbol` / 集合名 typo を明示エラー化済み |

## 式

| 構文 | 状態 | 備考 |
|------|------|------|
| `+ - * / ^`、括弧、単項マイナス | supported | Pratt パーサで AST 化 |
| 組み込み関数 `min max abs sqrt exp log pow` | supported | 引数個数（arity）も検証 |
| `sum{i in S} expr` / `sum(i in S) expr` | supported | 集合上の総和 |
| `if cond then a else b` | supported | 条件式（値）。`x <= if a then 1 else 2` のように制約の辺として使える |
| `if cond then <制約> else <制約>`（条件付き制約） | supported | 枝が制約（`y<=1` 等）の場合。cond は parse 時に評価し枝を選ぶ（param はインライン data）（#8） |
| ユーザー定義関数呼び出し `f(i)`（非組み込み） | planned/error | `unknown function 'f': ...` と明示エラー。`def` は未対応 |
| 制約集合の集約 `max(c in ...)` / `min(c in ...)` | planned | Issue #13 |
| 添字算術 `x[t-1]` / `x[t+1]` | planned | Issue #9 |

## 動的計画（DP）

| 構文 | 状態 | 備考 |
|------|------|------|
| `stage t in ...;` | partial | 集合として登録されるが DP 意味論は未評価 |
| `state S[t] in ...;` / `decision d[t] in ...;` | partial | 変数として登録されるが遷移/再帰は未評価 |
| `bellman ...` | planned/error | ベルマン再帰は未実装 |
| `transition: ...` | planned/error | 状態遷移は未実装 |
| `terminal ...` / `initial: ...` | planned/error | 端点条件は未実装 |

## その他の高度な機能

| 構文 | 状態 | 備考 |
|------|------|------|
| `def name(args) -> type: ...`（ユーザー定義関数） | planned/error | 未実装。呼び出しも上記の通りエラー |
| `import ...` | planned/unsupported | 外部モジュール/ML 埋め込みは未対応 |
| 確率計画 `prob[]` / シナリオ | planned | Fase2 |
| CP グローバル `disjunctive` / `no_overlap` / `cumulative` | partial | ペナルティ評価のみ。厳密解は `--features cp-sat`（実験的・未検証）+ OR-Tools 環境 |

## examples の現状

**現行実装で実行可能（golden）:**

- `f1_lp_production.optica` — 線形計画
- `f1_knapsack_binary.optica` — 0/1 ナップサック
- `f1_nlp_curve.optica` — 非線形（`^` 使用）
- `knapsack.optica` / `simple_knapsack.optica` — ナップサック

**`examples/experimental/` の `[EXPERIMENTAL]`（未対応構文・データ未整備のため現状では正しい結果を返さない。既定ではパースエラー、実行には `--allow-unsupported` が必要。各ファイル先頭に注記あり、Fase2 対象）:**

- `01_lp_production` `02_milp_facility` `03_nlp_portfolio` `04_convex_svm` `05_qp_regression`
- `06_dp_inventory` `07_stochastic_farmer` `08_combinatorial_tsp` `09_metaheuristic_vrp` `10_cp_scheduling`
- `11_moo_supply_chain` `12_ml_optimization` `13_largescale_decomposition`
- `advanced_features` `juku_timetabling`

> 実行可能サンプルは `examples/` 直下、将来仕様サンプルは `examples/experimental/` に分離済み（#3 完了条件②）。
