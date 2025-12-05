#!/usr/bin/env python3
"""
Optica Lexer デモスクリプト

Lexerの動作を確認するためのデモスクリプトです。
"""

import sys
from pathlib import Path

# プロジェクトルートをパスに追加
sys.path.insert(0, str(Path(__file__).parent))

from optica.lexer import tokenize, LexerError
from optica.tokens import TokenType


def demo_basic():
    """基本的なトークン化のデモ"""
    print("=" * 60)
    print("📝 Optica Lexer デモ")
    print("=" * 60)

    # シンプルな例
    examples = [
        ('集合定義', 'set STUDENTS = {"S1", "S2", "S3"}'),
        ('範囲集合', 'set SLOTS = 1..5'),
        ('パラメータ', 'param cost[ITEMS] real'),
        ('変数定義', 'var x[STUDENTS, SLOTS] binary'),
        ('目的関数', 'maximize profit:\n    sum(i in ITEMS) price[i] * x[i]'),
        ('制約', 'forall s in STUDENTS, k in SLOTS:\n    x[s,k] <= 1'),
    ]

    for name, source in examples:
        print(f"\n🔹 {name}")
        print(f"   入力: {source!r}")
        print("   トークン:")

        try:
            tokens = tokenize(source)
            for token in tokens:
                if token.type in (TokenType.NEWLINE, TokenType.INDENT, TokenType.DEDENT):
                    print(f"      [{token.type.name}]")
                elif token.type == TokenType.EOF:
                    print(f"      [{token.type.name}]")
                else:
                    print(f"      {token.type.name}: {token.value!r}")
        except LexerError as e:
            print(f"   ❌ エラー: {e}")


def demo_file(filepath: str):
    """ファイルをトークン化するデモ"""
    print("\n" + "=" * 60)
    print(f"📄 ファイルのトークン化: {filepath}")
    print("=" * 60)

    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            source = f.read()

        tokens = tokenize(source)

        # 統計情報
        type_counts: dict[str, int] = {}
        for token in tokens:
            type_name = token.type.name
            type_counts[type_name] = type_counts.get(type_name, 0) + 1

        print(f"\n📊 トークン統計:")
        print(f"   総トークン数: {len(tokens)}")
        print("\n   トークンタイプ別:")
        for type_name, count in sorted(type_counts.items(), key=lambda x: -x[1]):
            print(f"      {type_name}: {count}")

        # 重要なトークンを表示
        print("\n🔍 重要なトークン（最初の50個）:")
        count = 0
        for token in tokens:
            if token.type not in (TokenType.NEWLINE, TokenType.INDENT, TokenType.DEDENT, TokenType.EOF):
                print(f"   L{token.line:2}:{token.column:2} {token.type.name:12} {token.value!r}")
                count += 1
                if count >= 50:
                    print("   ... (省略)")
                    break

    except FileNotFoundError:
        print(f"❌ ファイルが見つかりません: {filepath}")
    except LexerError as e:
        print(f"❌ 字句解析エラー: {e}")


def main():
    """メイン関数"""
    demo_basic()

    # サンプルファイルがあればトークン化
    example_file = Path(__file__).parent / "examples" / "juku_timetabling.optica"
    if example_file.exists():
        demo_file(str(example_file))

    print("\n" + "=" * 60)
    print("✅ Lexer デモ完了！")
    print("=" * 60)


if __name__ == "__main__":
    main()

