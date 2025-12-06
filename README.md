# Optica

**Ultra-fast Optimization DSL**

純粋Rust実装。デフォルトは依存最小・ヒューリスティック。  
CP-SAT (OR-Tools) はオプション機能（`--features cp-sat`）で有効化。

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

## パフォーマンス（最新ベンチ、DE基準）

| 次元 | シングル(DE) | 並列(DE,10T) | 高速化 |
|------|--------------|--------------|--------|
| 100  | 14.6ms | **5.0ms** | 2.9x |
| 500  | 38.1ms | **14.7ms** | 2.6x |
| 1000 | 75.5ms | **28.6ms** | 2.6x |

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
├── parser.rs        # パーサー・式評価・MOO/CP記録・JSONロード
├── config.rs        # 定数
└── solver/
    ├── mod.rs       # ソルバー（DE/PSO/Hybrid、CPサポート入口）
    ├── rng.rs       # 乱数生成
    ├── objective.rs # 目的関数（デフォルトsphere）
    └── cpsat.rs     # CP-SAT連携（feature: cp-sat 時のみ）
```

## 特徴 / 制約

- **依存最小**: デフォルトは純Rustヒューリスティック。CP-SATはオプション。
- **CP-SAT**: `--features cp-sat` 時は OR-Tools の C++ 依存が必須（例: `brew install or-tools`）。依存が無い環境ではビルドエラーになります。
- **サイドカーJSON**: `model.optica` と同名の `model.json` を自動ロードしてパラメータ補完。
- **多目的**: 重み付き和 / epsilon をヒューリスティックで評価。
- **CPグローバル**: `disjunctive` / `no_overlap` / `cumulative` はペナルティ評価。厳密解は `--features cp-sat` + OR-Tools 環境で。
- **式パーサは簡易版**: 複雑な非線形/入れ子は0評価になる可能性。
- **JSONのみ対応**: 外部データ読み込みはJSONのサイドカーでのみサポート。
- **警告**: `sphere` 未使用などの警告が出る場合がありますが動作に影響はありません。

## ライセンス

MIT
