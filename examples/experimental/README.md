# examples/experimental/ — 将来仕様サンプル

このディレクトリのモデルは、現行 Optica が**未対応**の構文（ユーザー定義関数 `def`、DP系 `bellman`/`stage`/`state`/`transition`、確率計画 `prob[]`/シナリオ、CP系 `no_overlap`/`cumulative`、制約集合の集約 `max(c in ...)` など）を使用している、および/またはパラメータデータが未整備です。**現状では正しい結果を返しません**（各ファイル先頭に `[EXPERIMENTAL]` 注記あり。Fase2 で対応予定）。

未対応（planned）構文を含むため、既定では**パースエラー**になります。あくまで将来仕様の参考・実験用途で実行したい場合は `--allow-unsupported` を付けてください（未対応構文を警告してスキップし、残りだけを解きます — 結果は正しくない可能性があります）。

```bash
optica examples/experimental/06_dp_inventory.optica --allow-unsupported
```

実際に解ける最小サンプルは、ひとつ上の `examples/` 直下（`f1_lp_production.optica` など）を参照してください。構文ごとの対応状況は [../../docs/SPEC_SUPPORT.md](../../docs/SPEC_SUPPORT.md) を参照。
