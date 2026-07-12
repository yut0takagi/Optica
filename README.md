# Optica

**Ultra-fast Optimization DSL**

純粋Rust実装。デフォルトは依存最小・ヒューリスティック（差分進化/PSO/ハイブリッド）で、追加のネイティブ依存なしにビルド・実行できます。  
CP-SAT (OR-Tools) 連携はオプション機能（`--features cp-sat`）ですが、**実験的・未検証**です（下記「CP-SAT機能について」を参照）。

## インストール / ビルド

- ローカルビルド（デフォルト機能=ヒューリスティックのみ）

```bash
cargo build --release
```

- インストール（デフォルト機能のみ）

```bash
cargo install --path .
```

- CP-SATを有効化する場合（環境にOR-ToolsのC++依存が必要）

```bash
# OR-Toolsを用意（例: Homebrew）
brew install or-tools

# ビルド時にfeatureを有効化
cargo build --release --features cp-sat
```

> CP-SATの依存が整っていない環境で `--features cp-sat` を付けるとビルドが失敗します。デフォルト機能のみであれば純Rustでビルド可能です。
>
> **CP-SAT機能について**: `--features cp-sat` はネイティブの OR-Tools インストールを前提とする実験的機能で、本リポジトリの開発環境ではビルド・動作確認ができていません（未検証）。デフォルトビルド（純Rustヒューリスティック）のみが動作確認済みです。CP-SATを利用する場合は自己責任でビルド・検証してください。

```bash
# Rust必須
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# ヒューリスティックのみ（デフォルト）
cargo build --release

# CP-SATを有効化（OR-ToolsのC++依存が揃っている場合のみ）
cargo build --release --features cp-sat
```

## 使い方

```bash
# モデルを解く
optica model.optica

# オプション
optica model.optica -m de -i 2000 -t 8

# ベンチマーク
optica bench 100

# REPL
optica repl

# サイドカーJSONでパラメータを渡す（model.optica と同じ階層に model.json を置く）
optica model.optica
```

## 言語仕様

```optica
# ナップサック問題
set Items = {1, 2, 3, 4, 5};

param value[Items] = {1: 10, 2: 40, 3: 30, 4: 50, 5: 35};
param weight[Items] = {1: 5, 2: 4, 3: 6, 4: 3, 5: 2};

var x[Items] >= 0 <= 1;

maximize profit: sum{i in Items} value[i] * x[i];
subject to capacity: sum{i in Items} weight[i] * x[i] <= 10;
```

式はパース時に一度だけ AST（`src/expr.rs`、Pratt パーサ）にコンパイルされ、以後は再解析なしに評価されます。

- **演算子**: `+ - * / ^`（べき乗、右結合）、単項マイナス、`(...)` による優先順位制御
- **関数**: `min(a,b)` `max(a,b)` `abs(x)` `sqrt(x)` `exp(x)` `log(x)` `pow(a,b)`
- **集合和**: `sum{i in SET} expr` / `sum(i in SET) expr`（複数添字・カンマ区切りも可）
- **条件式**: `if <cond> then <expr> else <expr>`
- **複数行ブロック**: `maximize name:` / `minimize name:` / `subject to:` の本体を次の行以降にインデントして書けます
- **forall 制約展開**: `forall <i> in <SET>[, <j> in <SET2>]:` で集合の直積ぶん制約を自動展開
- **インライン制約**: `subject to name: <constraint>;` も1行で解釈・適用されます
- 明示的なパースエラーとして報告されるもの: 未知の関数名、未知のスカラーシンボル、未知の集合名（`sum{i in SET}` の `SET` を含む）、サポート外/不正な比較演算子を使う制約（`<=`/`>=`/`==` 以外、または `0 <= x <= 5` のような連鎖・重複比較）、および `subject to` ブロック外のトップレベル `forall`。これらは実行時に黙ってゼロ評価・無視されるのではなく、明示的なエラーとして報告されます。
  - **既知の残課題（Fase2 対象、現状はエラーにならず暗黙に 0 として評価されます）**: `x[j]` のような添字トークン内の未束縛変数名の typo は検証されません（`x[A]` のようなリテラル集合要素と区別できないため）。

### 未設定パラメータの診断

`param` で宣言されたのにデータ（値リテラル・`data:` ブロック・サイドカー JSON のいずれか）を一切与えられていないパラメータを、目的関数や制約が参照している場合、既定で**明示的なエラー**になります（暗黙 0 評価による偽の `Objective: 0` を防ぐため）。従来どおり未設定を 0 として続行したい場合は `--allow-missing-params` を渡すと、エラーの代わりに警告を出して解きます。値なしスカラー宣言 `param name real;` も既知シンボルとして登録されるため、参照しても `unknown symbol` にはなりません（データ未整備の場合は上記の未設定パラメータ診断が働きます）。

### 解ステータス表示

同梱のヒューリスティック（DE / PSO / hybrid）は一般に**最適性を証明しません**。そのため目的値が 0 に近くても `optimal` とは表示せず、次の語彙で解の性質を正直に表します。

- `heuristic_feasible`: 全制約を満たすヒューリスティック解（最適性の主張なし）
- `infeasible`: 制約違反が残る／実行可能解を見つけられなかった
- `feasible` / `optimal`: 最適性・実行可能性を証明できる backend（CP-SAT）由来。`optimal` は当該 backend が `OPTIMAL` を返した場合のみ

### 整数性（binary / int）

`binary` / `int` で宣言した変数は、丸め込み修復（各反復で最寄りの整数へ丸めてから評価）によって整数解に近づけます。これは**真の MILP（分枝限定法）ソルバーではなく、あくまでメタヒューリスティックによる近似**です。厳密な整数最適性を必要とする用途では `--features cp-sat`（実験的）の利用を検討してください。整数変数の判定はトークン単位で行われるため、たとえば `point` のような変数名が `int` の部分文字列と誤ってマッチすることはありません。

### 動作確認済み（golden）サンプル

以下は既知の最適値に対して golden テスト（`tests/golden.rs` ほか）で検証済みです。

| ファイル | 内容 | 既知最適 |
|----------|------|----------|
| `examples/knapsack.optica` | 容量制約付きナップサック（LP緩和） | 容量を守った上での最大利益 |
| `examples/simple_knapsack.optica` | 単純ナップサック（個数最大化） | count = 2 |
| `examples/f1_lp_production.optica` | 2製品・1資源のLP | 30 |
| `examples/f1_knapsack_binary.optica` | 0/1ナップサック | 220 |
| `examples/f1_nlp_curve.optica` | 非線形（`^`使用） | 1 |

```bash
cargo run --release -- examples/f1_knapsack_binary.optica -m de -i 2000
```

### 実験的（experimental）サンプル

[`examples/experimental/`](examples/experimental/) 配下のファイルは、現行 Optica が未対応の構文（制約集合の集約 `max(c in ...)` 等、`def`/`import`、DP系 `bellman`/`stage`/`state`、確率計画系 `prob[]`/シナリオ、CP系 `no_overlap`/`cumulative` など）を使用している、および/またはパラメータデータが未整備のため、**現状では正しい結果を返しません**（各ファイル先頭に `[EXPERIMENTAL]` コメントを付記済み。Fase2で対応予定）。未対応構文を含むため既定ではパースエラーになり、実行するには `--allow-unsupported` が必要です。

> 構文ごとの対応状況（supported / partial / planned / unsupported）と、未対応構文の既定エラー・`--allow-unsupported` の挙動は [docs/SPEC_SUPPORT.md](docs/SPEC_SUPPORT.md) を参照。

`01_lp_production` `02_milp_facility` `03_nlp_portfolio` `04_convex_svm` `05_qp_regression` `06_dp_inventory` `07_stochastic_farmer` `08_combinatorial_tsp` `09_metaheuristic_vrp` `10_cp_scheduling` `11_moo_supply_chain` `12_ml_optimization` `13_largescale_decomposition` `advanced_features` `juku_timetabling`（すべて `examples/experimental/`）

## パフォーマンス

最新のベンチマーク数値はここに固定値として掲載せず、お使いの環境で以下を実行して確認してください（マシン・ビルド設定により結果は変わります）。

```bash
optica bench 100
optica bench 500
optica bench 1000
```

シングルスレッド(DE)と並列(DE, マルチスレッド)の所要時間が表示されます。

## ソルバー

| メソッド | 特徴 |
|----------|------|
| `de` | 差分進化（デフォルト、並列対応） |
| `pso` | 粒子群最適化 |
| `hybrid` | DE + PSO ハイブリッド |

## プロジェクト構成

```
src/
├── main.rs          # CLI
├── cli.rs           # 引数解析
├── parser.rs        # パーサー（複数行ブロック/forall展開）・MOO/CP記録・JSONロード
├── expr.rs          # AST式評価器（Pratt パーサ、コンパイル済みExprを評価）
├── config.rs        # 定数
└── solver/
    ├── mod.rs       # ソルバー（DE/PSO/Hybrid、CPサポート入口）
    ├── rng.rs       # 乱数生成
    ├── objective.rs # 目的関数（デフォルトsphere）
    └── cpsat.rs     # CP-SAT連携（feature: cp-sat 時のみ）
```

## 特徴 / 制約

- **依存最小**: デフォルトは純Rustヒューリスティック。CP-SATはオプション。
- **CP-SAT**: `--features cp-sat` は**実験的・未検証**です。有効化時は OR-Tools の C++ 依存が必須（例: `brew install or-tools`）で、依存が無い環境ではビルドエラーになります。
- **サイドカーJSON**: `model.optica` と同名の `model.json` を自動ロードしてパラメータ補完。
- **多目的**: 重み付き和 / epsilon をヒューリスティックで評価。
- **CPグローバル**: `disjunctive` / `no_overlap` / `cumulative` はペナルティ評価。厳密解は `--features cp-sat`（実験的）+ OR-Tools 環境で。
- **整数性は近似**: `binary`/`int` は丸め込み修復による近似で、真のMILP（分枝限定法）ではありません。
- **式パーサはAST化済み**: `+ - * / ^`・関数（`min max abs sqrt exp log pow`）・`sum{..}`/`sum(..)`・`if..then..else`・複数行ブロック・`forall`展開に対応し、複雑な入れ子式も正しく評価されます。未対応なのは、制約集合の集約（`max(c in ...)` のような式）、`def`/`import`、DP（`bellman`/`stage`/`state`）・確率計画（`prob[]`/シナリオ）・CP（`no_overlap`/`cumulative`）の専用意味論です（Fase2対象）。
- **JSONのみ対応**: 外部データ読み込みはJSONのサイドカーでのみサポート。
- **警告**: `sphere` 未使用などの警告が出る場合がありますが動作に影響はありません。

## ライセンス

MIT
