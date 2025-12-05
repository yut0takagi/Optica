"""
Optica: A Domain-Specific Language for Constraint & Scheduling Optimization

Opticaは、スケジューリング・勤務表・コマ割り・資源配分などの
数理最適化問題を直感的に記述するためのDSLです。
"""

__version__ = "0.1.0"

from optica.lexer import Lexer
from optica.tokens import Token, TokenType

__all__ = ["Lexer", "Token", "TokenType", "__version__"]

