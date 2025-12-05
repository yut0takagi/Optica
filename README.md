# Optica: A Domain-Specific Language for Constraint & Scheduling Optimization

Optica（オプティカ）は、**スケジューリング・勤務表・コマ割り・資源配分などの数理最適化問題を直感的に記述するためのDSL（Domain Specific Language）**です。
本言語は、教育現場・業務効率化・研究用途において、非エンジニアでも最適化モデルを読み書きできることを目的としています。

Opticaで記述されたモデルは、内部で **Python（Pyomo / OR-Tools）に変換され、最適化ソルバー（CBC / GLPK / Gurobi 等）によって解かれます**。

---

# 1. 特徴

* 数理最適化（特にMILP）をシンプルに書ける独自言語
* 制約記述に特化した直感的な構文
* 集合、変数、目的関数、制約を簡潔に定義
* Pythonエンジンが裏側で自動的に最適解を算出
* CreateAppAutomateやAIエージェントによる自動生成と相性が良い
* 拡張子は **`.optica`**

---

# 2. Optica の基本構文

Optica のファイルは、以下の4つの主要ブロックで構成されます。

1. **set**（集合）
2. **param**（パラメータ）
3. **var**（変数）
4. **objective / constraints**（目的関数・制約）

以下に v0.1 仕様の文法をまとめます。

---

# 3. 言語仕様（v0.1）

## 3.1 コメント

```
# 行頭の # 以降はコメント
```

## 3.2 集合

```
set STUDENTS = {"S1", "S2", "S3"}
set TEACHERS = {"T1", "T2"}
set SLOTS    = 1..5
```

* `{}`：集合の列挙
* `a..b`：整数範囲

## 3.3 パラメータ

```
param can_teach[TEACHERS, STUDENTS] in {0,1}
param pref[STUDENTS, TEACHERS, SLOTS] real
```

* `in {0,1}`：離散値の指定
* `real / int`：型の指定

## 3.4 変数

```
var x[STUDENTS, TEACHERS, SLOTS] binary
```

* `binary`：0/1変数
* `int` / `real`：整数・実数変数

## 3.5 目的関数

```
maximize total_pref:
    sum(s in STUDENTS, t in TEACHERS, k in SLOTS)
        pref[s,t,k] * x[s,t,k]
```

* `maximize` / `minimize` のどちらかを選択
* `sum()` による線形式の表記

## 3.6 制約

```
subject to:

    each_student_one_per_slot:
        forall s in STUDENTS, k in SLOTS:
            sum(t in TEACHERS) x[s,t,k] <= 1

    each_teacher_one_per_slot:
        forall t in TEACHERS, k in SLOTS:
            sum(s in STUDENTS) x[s,t,k] <= 1

    teachable_only:
        forall s in STUDENTS, t in TEACHERS, k in SLOTS:
            x[s,t,k] <= can_teach[t,s]
```

* `forall` によるインデックス指定
* 制約名は任意

---

# 4. サンプルモデル：塾のコマ割り最適化

以下は Optica を用いた、教育現場向けの典型的な最適化モデルです。

```
model "Takagi Juku Timetabling"

set STUDENTS = {"S1", "S2", "S3"}
set TEACHERS = {"T1", "T2"}
set SLOTS    = 1..5

param pref[STUDENTS, TEACHERS, SLOTS] real
param can_teach[TEACHERS, STUDENTS] in {0,1}

var x[STUDENTS, TEACHERS, SLOTS] binary

maximize total_pref:
    sum(s in STUDENTS, t in TEACHERS, k in SLOTS)
        pref[s,t,k] * x[s,t,k]

subject to:

    each_student_one_per_slot:
        forall s in STUDENTS, k in SLOTS:
            sum(t in TEACHERS) x[s,t,k] <= 1

    each_teacher_one_per_slot:
        forall t in TEACHERS, k in SLOTS:
            sum(s in STUDENTS) x[s,t,k] <= 1

    teachable_only:
        forall s in STUDENTS, t in TEACHERS, k in SLOTS:
            x[s,t,k] <= can_teach[t,s]
```

---

# 5. 実行方法（v0.1 想定）

Optica は CLI から次のように使用する予定です。

```
optica solve model.optica --data data.json
```

1. `.optica` ファイルをパースし AST に変換
2. AST → Pyomo モデルに自動変換
3. CBC または Gurobi で最適化実行
4. 最適解を JSON または表形式で出力

---

# 6. プロジェクト構成案

```
optica/
  ├── parser/          # 字句解析・構文解析
  ├── ast/             # 抽象構文木
  ├── compiler/        # Pyomo/OR-Tools への変換
  ├── runtime/         # データ管理・型チェック
  ├── cli.py           # optica コマンド
  └── examples/        # サンプルモデル
```

---

# 7. 今後のロードマップ

### v0.2

* data ブロックの実装（Optica 内で値を記述可能に）
* VSCode 拡張のシンタックスハイライト
* sum/forall のネスト安全化

### v0.3

* ソルバー選択機能
* Solution Viewer（結果の可視化）
* CreateAppAutomate からの自動生成機能

### v1.0

* Optica → WASM 実行エンジン
* Web IDE
* モデル共有プラットフォーム

---

# 8. ライセンス

MIT License（予定）

---

# 9. 作者

**Takagi Yuto** — Optica 言語設計者

数理最適化・教育・AI 自動生成を融合した "未来のスケジューリング言語" を目指します。

---

# 10. コントリビュート

開発は随時進行中です。Issue・PR歓迎します。
