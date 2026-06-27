"""菱形左支：继承 Serializable。"""
from .base import Serializable


class AuthService(Serializable):
    def __init__(self, token: str):
        self.token = token

    def serialize(self) -> str:
        return self.token
