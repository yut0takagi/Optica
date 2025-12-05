"""
Optica 字句解析器（Lexer）

ソースコードをトークン列に変換します。
"""

from __future__ import annotations

from typing import Iterator

from optica.tokens import (
    KEYWORDS,
    SYMBOLS_DOUBLE,
    SYMBOLS_SINGLE,
    Token,
    TokenType,
)


class LexerError(Exception):
    """字句解析エラー"""

    def __init__(self, message: str, line: int, column: int):
        self.line = line
        self.column = column
        super().__init__(f"Lexer Error at L{line}:{column}: {message}")


class Lexer:
    """
    Optica言語の字句解析器

    ソースコードを読み込み、トークンのシーケンスを生成します。
    インデントベースの構文をサポートし、INDENT/DEDENTトークンを生成します。

    Usage:
        lexer = Lexer(source_code)
        tokens = lexer.tokenize()
    """

    def __init__(self, source: str):
        """
        Args:
            source: Opticaソースコード文字列
        """
        self.source = source
        self.pos = 0           # 現在位置
        self.line = 1          # 現在行（1始まり）
        self.column = 1        # 現在列（1始まり）
        self.indent_stack = [0]  # インデントレベルスタック
        self.tokens: list[Token] = []
        self.at_line_start = True  # 行頭フラグ

    @property
    def current_char(self) -> str | None:
        """現在位置の文字を返す（終端ならNone）"""
        if self.pos >= len(self.source):
            return None
        return self.source[self.pos]

    def peek(self, offset: int = 1) -> str | None:
        """先読み（offset文字先を返す）"""
        pos = self.pos + offset
        if pos >= len(self.source):
            return None
        return self.source[pos]

    def advance(self) -> str | None:
        """1文字進めて、その文字を返す"""
        char = self.current_char
        if char is None:
            return None

        self.pos += 1
        if char == "\n":
            self.line += 1
            self.column = 1
            self.at_line_start = True
        else:
            self.column += 1
            if char not in " \t":
                self.at_line_start = False

        return char

    def skip_whitespace(self) -> None:
        """空白をスキップ（改行は除く）"""
        while self.current_char is not None and self.current_char in " \t":
            self.advance()

    def skip_comment(self) -> None:
        """コメントをスキップ（# から行末まで）"""
        if self.current_char == "#":
            while self.current_char is not None and self.current_char != "\n":
                self.advance()

    def read_string(self) -> Token:
        """文字列リテラルを読み取る"""
        start_line = self.line
        start_column = self.column
        quote = self.current_char  # " または '

        self.advance()  # 開始クォートをスキップ
        value = ""

        while self.current_char is not None and self.current_char != quote:
            if self.current_char == "\\":
                # エスケープシーケンス
                self.advance()
                escape_char = self.current_char
                if escape_char == "n":
                    value += "\n"
                elif escape_char == "t":
                    value += "\t"
                elif escape_char == "\\":
                    value += "\\"
                elif escape_char == quote:
                    value += quote
                else:
                    value += escape_char or ""
                self.advance()
            elif self.current_char == "\n":
                raise LexerError("文字列リテラル内で改行が見つかりました", self.line, self.column)
            else:
                value += self.current_char
                self.advance()

        if self.current_char is None:
            raise LexerError("文字列リテラルが閉じられていません", start_line, start_column)

        self.advance()  # 終了クォートをスキップ

        return Token(
            type=TokenType.STRING,
            value=value,
            line=start_line,
            column=start_column,
            literal=f'{quote}{value}{quote}',
        )

    def read_number(self) -> Token:
        """数値リテラルを読み取る"""
        start_line = self.line
        start_column = self.column
        value = ""
        is_real = False

        # 整数部分
        while self.current_char is not None and self.current_char.isdigit():
            value += self.current_char
            self.advance()

        # 小数部分（.の後に数字が続く場合のみ、..は範囲演算子）
        if self.current_char == "." and self.peek() != ".":
            next_char = self.peek()
            if next_char is not None and next_char.isdigit():
                is_real = True
                value += self.current_char
                self.advance()
                while self.current_char is not None and self.current_char.isdigit():
                    value += self.current_char
                    self.advance()

        if is_real:
            return Token(
                type=TokenType.REAL,
                value=float(value),
                line=start_line,
                column=start_column,
                literal=value,
            )
        else:
            return Token(
                type=TokenType.INTEGER,
                value=int(value),
                line=start_line,
                column=start_column,
                literal=value,
            )

    def read_identifier_or_keyword(self) -> Token:
        """識別子またはキーワードを読み取る"""
        start_line = self.line
        start_column = self.column
        value = ""

        # 識別子: [a-zA-Z_][a-zA-Z0-9_]*
        while self.current_char is not None and (
            self.current_char.isalnum() or self.current_char == "_"
        ):
            value += self.current_char
            self.advance()

        # キーワードかどうかチェック
        token_type = KEYWORDS.get(value, TokenType.IDENTIFIER)

        return Token(
            type=token_type,
            value=value,
            line=start_line,
            column=start_column,
            literal=value,
        )

    def read_symbol(self) -> Token:
        """演算子・区切り文字を読み取る"""
        start_line = self.line
        start_column = self.column

        # 2文字演算子を先にチェック
        two_char = self.source[self.pos : self.pos + 2]
        if two_char in SYMBOLS_DOUBLE:
            self.advance()
            self.advance()
            return Token(
                type=SYMBOLS_DOUBLE[two_char],
                value=two_char,
                line=start_line,
                column=start_column,
                literal=two_char,
            )

        # 1文字演算子
        one_char = self.current_char
        if one_char in SYMBOLS_SINGLE:
            self.advance()
            return Token(
                type=SYMBOLS_SINGLE[one_char],
                value=one_char,
                line=start_line,
                column=start_column,
                literal=one_char,
            )

        raise LexerError(f"不明な文字: {one_char!r}", start_line, start_column)

    def handle_indentation(self) -> list[Token]:
        """
        行頭のインデントを処理し、INDENT/DEDENTトークンを生成

        Returns:
            生成されたINDENT/DEDENTトークンのリスト
        """
        tokens = []
        indent_level = 0
        start_column = self.column

        # インデントを計算（スペース = 1, タブ = 4として扱う）
        while self.current_char in " \t":
            if self.current_char == " ":
                indent_level += 1
            else:  # tab
                indent_level += 4
            self.advance()

        # 空行またはコメント行はインデント処理をスキップ
        if self.current_char == "\n" or self.current_char == "#":
            return tokens

        current_indent = self.indent_stack[-1]

        if indent_level > current_indent:
            # インデント増加
            self.indent_stack.append(indent_level)
            tokens.append(Token(
                type=TokenType.INDENT,
                value=indent_level,
                line=self.line,
                column=start_column,
                literal="",
            ))
        elif indent_level < current_indent:
            # インデント減少（複数のDEDENTが必要な場合がある）
            while self.indent_stack and indent_level < self.indent_stack[-1]:
                self.indent_stack.pop()
                tokens.append(Token(
                    type=TokenType.DEDENT,
                    value=indent_level,
                    line=self.line,
                    column=start_column,
                    literal="",
                ))

            # インデントレベルの整合性チェック
            if self.indent_stack and indent_level != self.indent_stack[-1]:
                raise LexerError(
                    f"不正なインデントレベル: {indent_level}（期待値: {self.indent_stack[-1]}）",
                    self.line,
                    start_column,
                )

        return tokens

    def next_token(self) -> Token | list[Token] | None:
        """
        次のトークンを返す

        Returns:
            Token, list[Token]（INDENT/DEDENT時）, または None（終端時）
        """
        # 行頭のインデント処理
        if self.at_line_start and self.current_char not in (None, "\n"):
            indent_tokens = self.handle_indentation()
            self.at_line_start = False
            if indent_tokens:
                return indent_tokens

        # 空白スキップ
        self.skip_whitespace()

        # コメントスキップ
        if self.current_char == "#":
            self.skip_comment()

        # 空白再スキップ（コメント後）
        self.skip_whitespace()

        # 終端チェック
        if self.current_char is None:
            # 残りのDEDENTを生成
            dedents = []
            while len(self.indent_stack) > 1:
                self.indent_stack.pop()
                dedents.append(Token(
                    type=TokenType.DEDENT,
                    value=0,
                    line=self.line,
                    column=self.column,
                    literal="",
                ))
            if dedents:
                return dedents
            return Token(
                type=TokenType.EOF,
                value=None,
                line=self.line,
                column=self.column,
                literal="",
            )

        # 改行
        if self.current_char == "\n":
            token = Token(
                type=TokenType.NEWLINE,
                value="\n",
                line=self.line,
                column=self.column,
                literal="\n",
            )
            self.advance()
            return token

        # 文字列
        if self.current_char in '"\'':
            return self.read_string()

        # 数値
        if self.current_char.isdigit():
            return self.read_number()

        # 識別子・キーワード
        if self.current_char.isalpha() or self.current_char == "_":
            return self.read_identifier_or_keyword()

        # 演算子・区切り文字
        return self.read_symbol()

    def tokenize(self) -> list[Token]:
        """
        ソースコード全体をトークン化

        Returns:
            トークンのリスト
        """
        tokens: list[Token] = []

        while True:
            result = self.next_token()

            if result is None:
                break

            if isinstance(result, list):
                tokens.extend(result)
            elif result.type == TokenType.EOF:
                tokens.append(result)
                break
            else:
                tokens.append(result)

        return tokens

    def tokenize_iter(self) -> Iterator[Token]:
        """
        トークンをイテレータとして返す

        Yields:
            Token
        """
        for token in self.tokenize():
            yield token


def tokenize(source: str) -> list[Token]:
    """
    ソースコードをトークン化するヘルパー関数

    Args:
        source: Opticaソースコード

    Returns:
        トークンのリスト
    """
    lexer = Lexer(source)
    return lexer.tokenize()

