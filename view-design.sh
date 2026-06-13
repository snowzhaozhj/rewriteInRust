#!/usr/bin/env bash
# 一键查看项目设计 HTML 报告
# 用法:
#   ./view-design.sh          # 打开 docs/design/index.html（完整设计文档 v0.9.4）
#   ./view-design.sh project  # 打开 PROJECT_DESIGN.html（项目设计概览 v0.8）
#
# 原因: 两个 HTML 用 fetch() 动态加载同目录的 .md 文件，
#       直接 file:// 双击会被浏览器 CORS 拦截（显示"无法加载"），
#       必须通过 HTTP 服务器访问。

set -euo pipefail

# 切到脚本所在目录（仓库根），保证相对路径正确
cd "$(dirname "$0")"

PORT=8765

case "${1:-design}" in
  project) URL_PATH="PROJECT_DESIGN.html" ;;
  design)  URL_PATH="docs/design/index.html" ;;
  *) echo "用法: $0 [design|project]" >&2; exit 1 ;;
esac

# 已有服务在跑就复用，否则后台启动
if ! curl -s -o /dev/null "http://localhost:${PORT}/" 2>/dev/null; then
  echo "启动本地 HTTP 服务 (端口 ${PORT})..."
  python3 -m http.server "${PORT}" --bind 127.0.0.1 >/dev/null 2>&1 &
  SERVER_PID=$!
  # 等服务就绪
  for _ in $(seq 1 20); do
    curl -s -o /dev/null "http://localhost:${PORT}/" 2>/dev/null && break
    sleep 0.2
  done
  echo "服务 PID=${SERVER_PID}（关闭: kill ${SERVER_PID}）"
fi

URL="http://localhost:${PORT}/${URL_PATH}"
echo "正在打开: ${URL}"
open "${URL}"
