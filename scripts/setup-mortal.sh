#!/usr/bin/env bash
# 在 mortal/ 中准备本地 CPU 推理环境；源码 checkout、虚拟环境和权重不进入 Git。
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
python_bin="${1:-python3}"
runtime_dir="$project_dir/mortal/runtime"
venv_dir="$project_dir/mortal/.venv"
checkpoint="$project_dir/mortal/models/mortal_582500.pth"
runtime_commit=0cff2b52982be5b1163aa9a62fb01f03ce91e0d2
model_sha=738e0d6e3c0ce9671629554ad39abd147d2ffbac676e80b194c83f2acc0fea20

"$python_bin" -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ is required (3.12 recommended)"'
mkdir -p mortal/models
if [[ ! -d "$venv_dir" ]]; then
    "$python_bin" -m venv "$venv_dir"
fi
"$venv_dir/bin/python" -m pip install 'torch==2.14.0' 'numpy==2.5.2'

if [[ ! -d "$runtime_dir" ]]; then
    git clone https://github.com/Equim-chan/Mortal.git "$runtime_dir"
    git -C "$runtime_dir" checkout --detach "$runtime_commit"
fi
if [[ "$(git -C "$runtime_dir" rev-parse HEAD)" != "$runtime_commit" ]]; then
    echo "Unexpected Mortal revision in $runtime_dir; expected $runtime_commit" >&2
    exit 1
fi
(
    cd "$runtime_dir"
    PYO3_PYTHON="$venv_dir/bin/python" cargo build -p libriichi --lib --release --locked
)
case "$(uname -s)" in
    Darwin) extension=libriichi.dylib ;;
    Linux) extension=libriichi.so ;;
    *) echo 'This setup script supports macOS and Linux.' >&2; exit 1 ;;
esac
cp "$runtime_dir/target/release/$extension" "$runtime_dir/mortal/libriichi.so"

if [[ ! -f "$checkpoint" ]]; then
    curl -fL --retry 3 -C - \
        https://huggingface.co/Yuchen1457/mortal-582500/resolve/7386c9f5c751a3ea75efea99737cef5a5ef950f1/mortal_582500.pth \
        -o "$checkpoint.part"
    "$venv_dir/bin/python" - "$checkpoint.part" "$model_sha" <<'PY'
import hashlib
import sys
with open(sys.argv[1], "rb") as source:
    digest = hashlib.file_digest(source, "sha256").hexdigest()
if digest != sys.argv[2]:
    raise SystemExit(f"Checkpoint checksum mismatch: {digest}")
PY
    mv "$checkpoint.part" "$checkpoint"
fi
"$venv_dir/bin/python" - "$checkpoint" "$model_sha" <<'PY'
import hashlib
import sys
with open(sys.argv[1], "rb") as source:
    digest = hashlib.file_digest(source, "sha256").hexdigest()
if digest != sys.argv[2]:
    raise SystemExit(f"Checkpoint checksum mismatch: {digest}")
print(f"Mortal checkpoint verified: {digest}")
PY
echo 'Ready: cargo run --bin mortal -- --player 0 --event 2 fixtures/tenhou/ranked_game.json'
