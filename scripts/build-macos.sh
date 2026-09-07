#!/usr/bin/env bash
# 先准备可移动的推理资源，再构建完整安装包；开发时仍可直接使用 tauri dev。
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    echo '当前打包流程仅支持 Apple Silicon macOS。' >&2
    exit 1
fi
"$project_dir/mortal/.venv/bin/python" scripts/prepare-macos.py
"$project_dir/mortal/.venv/bin/python" scripts/prepare-majsoul.py
MACOSX_DEPLOYMENT_TARGET=14.0 npm --prefix desktop run tauri -- build --config src-tauri/tauri.bundle.conf.json
"$project_dir/mortal/.venv/bin/python" scripts/prepare-macos.py --verify \
    "$project_dir/desktop/src-tauri/target/release/bundle/macos/Kyoku.app/Contents/Resources/inference"
"$project_dir/mortal/.venv/bin/python" scripts/prepare-majsoul.py --verify \
    "$project_dir/desktop/src-tauri/target/release/bundle/macos/Kyoku.app/Contents/Resources/majsoul"
