"""菱形顶点：抽象基类，被 auth 和 user 共同继承。"""
from abc import ABC, abstractmethod

__all__ = ["Serializable"]


class Serializable(ABC):
    @abstractmethod
    def serialize(self) -> str:
        ...
