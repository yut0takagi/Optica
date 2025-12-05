"""
Optica Lexer テスト
"""

import pytest

from optica.lexer import Lexer, LexerError, tokenize
from optica.tokens import Token, TokenType


class TestLexerBasics:
    """基本的なトークン化のテスト"""

    def test_empty_source(self):
        """空のソースコード"""
        tokens = tokenize("")
        assert len(tokens) == 1
        assert tokens[0].type == TokenType.EOF

    def test_keywords(self):
        """キーワードの認識"""
        source = "model set param var maximize minimize forall sum in binary int real"
        tokens = tokenize(source)

        expected_types = [
            TokenType.MODEL, TokenType.SET, TokenType.PARAM, TokenType.VAR,
            TokenType.MAXIMIZE, TokenType.MINIMIZE, TokenType.FORALL,
            TokenType.SUM, TokenType.IN, TokenType.BINARY, TokenType.INT,
            TokenType.REAL_TYPE, TokenType.EOF
        ]

        for token, expected in zip(tokens, expected_types):
            assert token.type == expected

    def test_integers(self):
        """整数リテラル"""
        source = "1 42 100 0"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.INTEGER
        assert tokens[0].value == 1

        assert tokens[1].type == TokenType.INTEGER
        assert tokens[1].value == 42

        assert tokens[2].type == TokenType.INTEGER
        assert tokens[2].value == 100

        assert tokens[3].type == TokenType.INTEGER
        assert tokens[3].value == 0

    def test_real_numbers(self):
        """実数リテラル"""
        source = "3.14 0.5 10.0"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.REAL
        assert tokens[0].value == 3.14

        assert tokens[1].type == TokenType.REAL
        assert tokens[1].value == 0.5

        assert tokens[2].type == TokenType.REAL
        assert tokens[2].value == 10.0

    def test_strings(self):
        """文字列リテラル"""
        source = '"S1" "Teacher" "Hello World"'
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.STRING
        assert tokens[0].value == "S1"

        assert tokens[1].type == TokenType.STRING
        assert tokens[1].value == "Teacher"

        assert tokens[2].type == TokenType.STRING
        assert tokens[2].value == "Hello World"

    def test_identifiers(self):
        """識別子"""
        source = "STUDENTS x pref_value _private var1"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.IDENTIFIER
        assert tokens[0].value == "STUDENTS"

        assert tokens[1].type == TokenType.IDENTIFIER
        assert tokens[1].value == "x"

        assert tokens[2].type == TokenType.IDENTIFIER
        assert tokens[2].value == "pref_value"

        assert tokens[3].type == TokenType.IDENTIFIER
        assert tokens[3].value == "_private"

        assert tokens[4].type == TokenType.IDENTIFIER
        assert tokens[4].value == "var1"


class TestOperators:
    """演算子のテスト"""

    def test_comparison_operators(self):
        """比較演算子"""
        source = "= == != < > <= >="
        tokens = tokenize(source)

        expected = [
            TokenType.EQ, TokenType.EQEQ, TokenType.NEQ,
            TokenType.LT, TokenType.GT, TokenType.LE, TokenType.GE, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected):
            assert token.type == exp

    def test_arithmetic_operators(self):
        """算術演算子"""
        source = "+ - * /"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.PLUS
        assert tokens[1].type == TokenType.MINUS
        assert tokens[2].type == TokenType.STAR
        assert tokens[3].type == TokenType.SLASH

    def test_range_operator(self):
        """範囲演算子"""
        source = "1..5"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.INTEGER
        assert tokens[0].value == 1
        assert tokens[1].type == TokenType.DOTDOT
        assert tokens[2].type == TokenType.INTEGER
        assert tokens[2].value == 5


class TestDelimiters:
    """区切り文字のテスト"""

    def test_braces(self):
        """括弧"""
        source = "{ } [ ] ( )"
        tokens = tokenize(source)

        expected = [
            TokenType.LBRACE, TokenType.RBRACE,
            TokenType.LBRACKET, TokenType.RBRACKET,
            TokenType.LPAREN, TokenType.RPAREN, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected):
            assert token.type == exp

    def test_comma_colon(self):
        """カンマとコロン"""
        source = ", :"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.COMMA
        assert tokens[1].type == TokenType.COLON


class TestComments:
    """コメントのテスト"""

    def test_line_comment(self):
        """行コメント"""
        source = "set # this is a comment\nvar"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.SET
        assert tokens[1].type == TokenType.NEWLINE
        assert tokens[2].type == TokenType.VAR

    def test_only_comment(self):
        """コメントのみ"""
        source = "# this is a comment"
        tokens = tokenize(source)

        # コメントはスキップされるのでEOFのみ
        assert tokens[0].type == TokenType.EOF


class TestSetDefinition:
    """集合定義のテスト"""

    def test_set_with_strings(self):
        """文字列集合"""
        source = 'set STUDENTS = {"S1", "S2", "S3"}'
        tokens = tokenize(source)

        expected_types = [
            TokenType.SET, TokenType.IDENTIFIER, TokenType.EQ,
            TokenType.LBRACE, TokenType.STRING, TokenType.COMMA,
            TokenType.STRING, TokenType.COMMA, TokenType.STRING,
            TokenType.RBRACE, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected_types):
            assert token.type == exp

    def test_set_with_range(self):
        """範囲集合"""
        source = "set SLOTS = 1..5"
        tokens = tokenize(source)

        expected_types = [
            TokenType.SET, TokenType.IDENTIFIER, TokenType.EQ,
            TokenType.INTEGER, TokenType.DOTDOT, TokenType.INTEGER, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected_types):
            assert token.type == exp


class TestComplexExpressions:
    """複雑な式のテスト"""

    def test_param_definition(self):
        """パラメータ定義"""
        source = "param pref[STUDENTS, TEACHERS, SLOTS] real"
        tokens = tokenize(source)

        expected_types = [
            TokenType.PARAM, TokenType.IDENTIFIER,
            TokenType.LBRACKET, TokenType.IDENTIFIER, TokenType.COMMA,
            TokenType.IDENTIFIER, TokenType.COMMA, TokenType.IDENTIFIER,
            TokenType.RBRACKET, TokenType.REAL_TYPE, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected_types):
            assert token.type == exp

    def test_var_definition(self):
        """変数定義"""
        source = "var x[STUDENTS, TEACHERS, SLOTS] binary"
        tokens = tokenize(source)

        expected_types = [
            TokenType.VAR, TokenType.IDENTIFIER,
            TokenType.LBRACKET, TokenType.IDENTIFIER, TokenType.COMMA,
            TokenType.IDENTIFIER, TokenType.COMMA, TokenType.IDENTIFIER,
            TokenType.RBRACKET, TokenType.BINARY, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected_types):
            assert token.type == exp

    def test_sum_expression(self):
        """sum式"""
        source = "sum(s in STUDENTS) x[s]"
        tokens = tokenize(source)

        expected_types = [
            TokenType.SUM, TokenType.LPAREN,
            TokenType.IDENTIFIER, TokenType.IN, TokenType.IDENTIFIER,
            TokenType.RPAREN, TokenType.IDENTIFIER,
            TokenType.LBRACKET, TokenType.IDENTIFIER, TokenType.RBRACKET, TokenType.EOF
        ]

        for token, exp in zip(tokens, expected_types):
            assert token.type == exp


class TestIndentation:
    """インデントのテスト"""

    def test_simple_indent(self):
        """シンプルなインデント"""
        source = "block:\n    content"
        tokens = tokenize(source)

        # block : NEWLINE INDENT content
        types = [t.type for t in tokens]
        assert TokenType.INDENT in types

    def test_dedent(self):
        """デデント"""
        source = "block:\n    content\nother"
        tokens = tokenize(source)

        types = [t.type for t in tokens]
        assert TokenType.INDENT in types
        assert TokenType.DEDENT in types


class TestLineInfo:
    """行・列情報のテスト"""

    def test_line_numbers(self):
        """行番号の追跡"""
        source = "line1\nline2\nline3"
        tokens = tokenize(source)

        assert tokens[0].line == 1  # line1
        assert tokens[2].line == 2  # line2
        assert tokens[4].line == 3  # line3

    def test_column_numbers(self):
        """列番号の追跡"""
        source = "ab cd ef"
        tokens = tokenize(source)

        assert tokens[0].column == 1  # ab
        assert tokens[1].column == 4  # cd
        assert tokens[2].column == 7  # ef


class TestErrors:
    """エラーハンドリングのテスト"""

    def test_unclosed_string(self):
        """閉じられていない文字列"""
        source = '"unclosed string'
        with pytest.raises(LexerError):
            tokenize(source)

    def test_unknown_character(self):
        """不明な文字"""
        source = "set @invalid"
        with pytest.raises(LexerError):
            tokenize(source)


class TestFullModel:
    """完全なモデルのテスト"""

    def test_model_header(self):
        """モデルヘッダー"""
        source = 'model "Test Model"'
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.MODEL
        assert tokens[1].type == TokenType.STRING
        assert tokens[1].value == "Test Model"

    def test_subject_to(self):
        """subject to キーワード"""
        source = "subject to:"
        tokens = tokenize(source)

        assert tokens[0].type == TokenType.SUBJECT
        assert tokens[1].type == TokenType.TO
        assert tokens[2].type == TokenType.COLON


if __name__ == "__main__":
    pytest.main([__file__, "-v"])

