# Changelog

## Unreleased - Fase1: Trustworthy Core
- feat(expr): 式評価器を文字列の毎回再解析からAST評価（`src/expr.rs`, Pratt パーサ）へ全面書き換え。`+ - * / ^`、単項マイナス、括弧、関数 `min max abs sqrt exp log pow`、`sum{..}`/`sum(..)`、`if..then..else` に対応。未知の関数名・記号や構文エラーは黙って0を返さず、明示的なパースエラーになる。
- feat(parser): 複数行の `maximize name:` / `minimize name:` / `subject to:` ブロック（本体を後続のインデント行に記述）に対応。
- feat(parser): `forall <i> in <SET>[, ...]:` による制約の集合展開に対応。
- feat(parser): インラインの `subject to name: <constraint>` を正しくパースし適用するように修正（従来は無言で無視されていた）。
- fix(solver): 早期収束バグを修正。`best_fit < TOLERANCE` による早期終了（最大化・負の目的値の問題でおよそ1反復で打ち切られていた）を、改善が一定反復数見られない場合に停止する停滞ベースの停止条件に置換。最大化が正しく動作するようになった。
- feat(solver): `binary`/`int` 変数の整数性を丸め込み修復で実現（近似であり、真のMILP/分枝限定法ソルバーではない）。整数変数の判定はトークン単位で行い、`point` のような変数名を `int` と誤マッチしないようにした。
- test: data内蔵のgoldenサンプル（既知最適値を持つ）と、パーサ/ソルバー修正の回帰テストを追加。CI（fmt/clippy/test）がテスト0件の状態から実質的にグリーンになった。
- chore: リポジトリ内に誤って追跡されていたバイナリを削除し、`target/` の追跡を停止。

## 1.0.0 - 2025-12-06
- 初版公開。差分進化(DE)・PSO・ハイブリッドのヒューリスティックソルバーを同梱。
- CP-SAT連携をオプション機能（`--features cp-sat`）として提供。OR-ToolsのC++依存が必要。
- DSLパーサ/サイドカーJSON読み込み/多目的（weighted sum・epsilon制約）対応を整備。
- CI（fmt/clippy/test）を追加しデフォルト機能での品質ゲートを構築。

