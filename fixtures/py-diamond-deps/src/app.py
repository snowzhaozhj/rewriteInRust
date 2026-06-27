"""菱形汇聚点：同时依赖 auth 和 user（→ base）。"""
from .auth import AuthService
from .user import UserModel


def build():
    auth = AuthService("tok")
    user = UserModel("alice")
    return auth.serialize() + user.serialize()
