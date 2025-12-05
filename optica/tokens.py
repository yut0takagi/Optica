"""
Optica トークン定義

Optica言語で使用されるすべてのトークンタイプと
トークンクラスを定義します。
"""

from dataclasses import dataclass
from enum import Enum, auto
from typing import Any


class TokenType(Enum):
    """Optica言語のトークンタイプ"""

    # ========== リテラル ==========
    INTEGER = auto()      # 整数: 1, 42, 100
    REAL = auto()         # 実数: 3.14, 0.5
    STRING = auto()       # 文字列: "S1", "Teacher"

    # ========== 識別子 ==========
    IDENTIFIER = auto()   # 識別子: STUDENTS, x, pref

    # ========== キーワード ==========
    MODEL = auto()        # model
    SET = auto()          # set
    PARAM = auto()        # param
    VAR = auto()          # var
    MAXIMIZE = auto()     # maximize
    MINIMIZE = auto()     # minimize
    SUBJECT = auto()      # subject
    TO = auto()           # to
    FORALL = auto()       # forall
    SUM = auto()          # sum
    IN = auto()           # in
    BINARY = auto()       # binary
    INT = auto()          # int
    REAL_TYPE = auto()    # real (型として)

    # ========== 比較演算子 ==========
    EQ = auto()           # =
    EQEQ = auto()         # ==
    NEQ = auto()          # !=
    LT = auto()           # <
    GT = auto()           # >
    LE = auto()           # <=
    GE = auto()           # >=

    # ========== 算術演算子 ==========
    PLUS = auto()         # +
    MINUS = auto()        # -
    STAR = auto()         # *
    SLASH = auto()        # /

    # ========== 範囲演算子 ==========
    DOTDOT = auto()       # ..

    # ========== 区切り文字 ==========
    LBRACE = auto()       # {
    RBRACE = auto()       # }
    LBRACKET = auto()     # [
    RBRACKET = auto()     # ]
    LPAREN = auto()       # (
    RPAREN = auto()       # )
    COMMA = auto()        # ,
    COLON = auto()        # :

    # ========== 特殊トークン ==========
    NEWLINE = auto()      # 改行
    EOF = auto()          # ファイル終端
    INDENT = auto()       # インデント増加
    DEDENT = auto()       # インデント減少

    # ========== コメント（通常はスキップ） ==========
    COMMENT = auto()      # # コメント


# キーワードマッピング
KEYWORDS: dict[str, TokenType] = {
    "model": TokenType.MODEL,
    "set": TokenType.SET,
    "param": TokenType.PARAM,
    "var": TokenType.VAR,
    "maximize": TokenType.MAXIMIZE,
    "minimize": TokenType.MINIMIZE,
    "subject": TokenType.SUBJECT,
    "to": TokenType.TO,
    "forall": TokenType.FORALL,
    "sum": TokenType.SUM,
    "in": TokenType.IN,
    "binary": TokenType.BINARY,
    "int": TokenType.INT,
    "real": TokenType.REAL_TYPE,
}


@dataclass
class Token:
    """
    トークンを表すクラス

    Attributes:
        type: トークンの種類
        value: トークンの値（リテラル値や識別子名）
        line: ソースコード内の行番号（1始まり）
        column: ソースコード内の列番号（1始まり）
        literal: 元のソースコード文字列
    """

    type: TokenType
    value: Any
    line: int
    column: int
    literal: str = ""

    def __repr__(self) -> str:
        if self.value is not None:
            return f"Token({self.type.name}, {self.value!r}, L{self.line}:{self.column})"
        return f"Token({self.type.name}, L{self.line}:{self.column})"

    def __str__(self) -> str:
        return self.__repr__()


# シンボルマッピング（1文字 → 2文字を優先してチェック）
SYMBOLS_DOUBLE: dict[str, TokenType] = {
    "..": TokenType.DOTDOT,
    "<=": TokenType.LE,
    ">=": TokenType.GE,
    "==": TokenType.EQEQ,
    "!=": TokenType.NEQ,
}

SYMBOLS_SINGLE: dict[str, TokenType] = {
    "=": TokenType.EQ,
    "<": TokenType.LT,
    ">": TokenType.GT,
    "+": TokenType.PLUS,
    "-": TokenType.MINUS,
    "*": TokenType.STAR,
    "/": TokenType.SLASH,
    "{": TokenType.LBRACE,
    "}": TokenType.RBRACE,
    "[": TokenType.LBRACKET,
    "]": TokenType.RBRACKET,
    "(": TokenType.LPAREN,
    ")": TokenType.RPAREN,
    ",": TokenType.COMMA,
    ":": TokenType.COLON,
}

