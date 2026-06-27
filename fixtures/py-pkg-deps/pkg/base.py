"""抽象基类。含 if TYPE_CHECKING 块（仅类型用途的导入，不构成运行时依赖）。"""
from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .types import Config


class Base(ABC):
    @abstractmethod
    def run(self) -> int:
        ...

    @property
    def label(self) -> str:
        return "base"

    def configure(self, cfg: "Config") -> None:
        ...
