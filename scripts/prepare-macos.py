#!/usr/bin/env python3
"""把已验证的 Mortal 环境整理成可随应用移动的 macOS 资源，不下载或修改开发环境。"""

import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile

PROJECT = Path(__file__).resolve().parent.parent
COMMIT = "0cff2b52982be5b1163aa9a62fb01f03ce91e0d2"
MODEL_SHA = "738e0d6e3c0ce9671629554ad39abd147d2ffbac676e80b194c83f2acc0fea20"
MACHO = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs).stdout.strip()


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def ignored(directory, names):
    return [name for name in names if name == "__pycache__" or name.endswith(".pyc")]


def tree_digest(root, exclude_site=False):
    result = hashlib.sha256()
    for directory, dirs, files in os.walk(root):
        dirs[:] = sorted(name for name in dirs if name not in ("__pycache__", "pkgconfig") and not (exclude_site and name == "site-packages"))
        for name in sorted(files):
            if name.endswith(".pyc"):
                continue
            path = Path(directory) / name
            result.update(str(path.relative_to(root)).encode())
            result.update(digest(path).encode())
    return result.hexdigest()


def native_files(root):
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            with path.open("rb") as source:
                if source.read(4) in MACHO:
                    yield path


def audit_native(root):
    """不能依赖构建机器上的 Homebrew、虚拟环境或外部动态库。"""
    for path in root.rglob("*"):
        if path.is_symlink() and not path.resolve().is_relative_to(root.resolve()):
            raise RuntimeError(f"资源符号链接指向包外：{path}")
    count = 0
    for path in native_files(root):
        count += 1
        def expand(value):
            return Path(value.replace("@loader_path", str(path.parent))
                        .replace("@executable_path", str(root / "python/bin"))).resolve()

        commands = run("otool", "-l", str(path)).splitlines()
        rpaths = [commands[i + 2].strip().removeprefix("path ").split(" (offset")[0]
                  for i, line in enumerate(commands) if line.strip() == "cmd LC_RPATH"]
        for rpath in rpaths:
            if not expand(rpath).is_relative_to(root.resolve()):
                raise RuntimeError(f"动态库搜索路径指向包外：{path.name}: {rpath}")
        identities = run("otool", "-D", str(path)).splitlines()[1:]
        for line in run("otool", "-L", str(path)).splitlines()[1:]:
            dependency = line.strip().split(" (compatibility version")[0]
            if dependency in identities or dependency.startswith(("/usr/lib/", "/System/Library/")):
                continue
            if dependency.startswith("@rpath/"):
                candidates = [expand(rpath) / dependency.removeprefix("@rpath/") for rpath in rpaths]
                candidates.append(root / "python/lib" / dependency.removeprefix("@rpath/"))
            elif dependency.startswith(("@loader_path/", "@executable_path/")):
                candidates = [expand(dependency)]
            else:
                candidates = []
            if any(candidate.is_file() and candidate.resolve().is_relative_to(root.resolve()) for candidate in candidates):
                continue
            raise RuntimeError(f"发现不可移植的动态库依赖：{path.name}: {dependency}")
    return count


def verify(root):
    if digest(root / "models/mortal_582500.pth") != MODEL_SHA:
        raise RuntimeError("打包权重 SHA-256 不匹配")
    python = root / "python/bin/python3"
    environment = {"PATH": "/usr/bin:/bin", "HOME": str(root), "PYTHONHOME": "/invalid-python-home", "PYTHONPATH": "/invalid-python-path"}
    # 同应用使用相同桥接脚本；加载权重并实际处理示例牌谱，避免只检查 import 成功。
    events = [
        {"type": "start_game", "names": ["A", "B", "C", "D"]},
        {"type": "start_kyoku", "bakaze": "E", "dora_marker": "1p", "kyoku": 1,
         "honba": 0, "kyotaku": 0, "oya": 0, "scores": [25000] * 4,
         "tehais": [["1m", "2m", "3m", "4m", "5m", "6m", "7p", "8p", "9p", "E", "E", "P", "P"]] + [["?"] * 13] * 3},
        {"type": "tsumo", "actor": 0, "pai": "1s"},
    ]
    output = run(str(python), "-I", "-B", "-u", "-c", (PROJECT / "src/mortal/bridge.py").read_text(),
                 str(root / "runtime"), str(root / "models/mortal_582500.pth"), "0",
                 input="".join(json.dumps(event) + "\n" for event in events), env=environment, cwd="/", timeout=90)
    lines = [json.loads(line) for line in output.splitlines()]
    if (len(lines) != 4 or lines[0]["sha256"] != MODEL_SHA or lines[0]["version"] != 4
            or lines[-1]["type"] not in ("dahai", "reach") or not lines[-1].get("meta", {}).get("mask_bits")):
        raise RuntimeError(f"打包环境未能完成真实切牌推理：{[line.get('type', 'model') for line in lines]}")


def prepare(output):
    if sys.platform != "darwin" or platform.machine() != "arm64":
        raise RuntimeError("当前安装包仅支持在 Apple Silicon Mac 上构建")
    if sys.version_info[:2] != (3, 12) or sys.prefix == sys.base_prefix:
        raise RuntimeError("请使用已准备好的 Python 3.12 Mortal 虚拟环境运行本脚本")
    base = Path(sys.base_prefix)
    packages = Path(sys.prefix) / "lib/python3.12/site-packages"
    runtime = PROJECT / "mortal/runtime"
    model = PROJECT / "mortal/models/mortal_582500.pth"
    extension = runtime / "mortal/libriichi.so"
    if run("git", "-C", str(runtime), "rev-parse", "HEAD") != COMMIT:
        raise RuntimeError("Mortal 源码版本与固定版本不符")
    if run("git", "-C", str(runtime), "status", "--porcelain=v1", "--untracked-files=all"):
        raise RuntimeError("Mortal 源码存在本地修改，请先处理后再打包")
    if digest(model) != MODEL_SHA:
        raise RuntimeError("Mortal 权重校验失败")
    versions = {name: importlib.metadata.version(name) for name in ("torch", "numpy")}
    if versions != {"torch": "2.14.0", "numpy": "2.5.2"}:
        raise RuntimeError("请先用 setup-mortal.sh 准备固定版本的 PyTorch 和 NumPy")
    inputs = {
        "script": digest(Path(__file__)), "bridge": digest(PROJECT / "src/mortal/bridge.py"),
        "python": platform.python_version(), "architecture": platform.machine(),
        "executable": digest(base / "bin/python3.12"), "stdlib": tree_digest(base / "lib", exclude_site=True),
        "packages": tree_digest(packages), "extension": digest(extension),
        "runtime_commit": COMMIT, "model_sha256": MODEL_SHA,
        "source_notes": digest(PROJECT / "mortal/README.md"),
        "model_notes": {name: digest(PROJECT / "mortal/models" / name) for name in ("MODEL_CARD.md", "Mortal-LICENSE", "model-manifest.json")},
        **versions,
    }
    print("已校验源环境，正在准备独立资源…", flush=True)
    manifest = output / "manifest.json"
    if manifest.is_file() and json.loads(manifest.read_text()).get("inputs") == inputs:
        print(f"复用已准备的推理环境：{output}", flush=True)
        verify(output)
        return
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="kyoku-runtime-", dir=output.parent) as temporary:
        stage = Path(temporary) / "inference"
        python = stage / "python"
        (python / "bin").mkdir(parents=True)
        shutil.copy2(base / "bin/python3.12", python / "bin/python3")

        def base_ignore(directory, names):
            return ignored(directory, names) + [name for name in names if name in ("site-packages", "pkgconfig")]

        shutil.copytree(base / "lib", python / "lib", ignore=base_ignore)
        shutil.copytree(packages, python / "lib/python3.12/site-packages", ignore=ignored)
        (stage / "runtime").mkdir()
        archive = Path(temporary) / "mortal.tar"
        subprocess.run(["git", "-C", str(runtime), "archive", "--format=tar", f"--output={archive}", COMMIT], check=True)
        run("tar", "-xf", str(archive), "-C", str(stage / "runtime"))
        shutil.copy2(extension, stage / "runtime/mortal/libriichi.so")
        run("install_name_tool", "-id", "@rpath/libriichi.so", str(stage / "runtime/mortal/libriichi.so"))
        (stage / "models").mkdir()
        for name in ("mortal_582500.pth", "MODEL_CARD.md", "Mortal-LICENSE", "model-manifest.json"):
            shutil.copy2(PROJECT / "mortal/models" / name, stage / "models" / name)
        shutil.copy2(PROJECT / "mortal/README.md", stage / "Mortal-SOURCES.md")
        print("正在检查动态库并签署本地资源…", flush=True)
        for path in native_files(stage):
            if len(run("otool", "-D", str(path)).splitlines()) > 1:
                run("install_name_tool", "-id", f"@rpath/{path.name}", str(path))
        count = audit_native(stage)
        # 修改动态库标识会使原签名失效；先逐个签署资源，外层应用由 Tauri 签署。
        for path in native_files(stage):
            run("codesign", "--force", "--sign", "-", str(path))
        print("正在验证独立环境中的模型推理…", flush=True)
        verify(stage)
        (stage / "manifest.json").write_text(json.dumps({"inputs": inputs, "native_files": count}, indent=2) + "\n")
        # 只替换脚本自己的产物；失败时保留上一次已验证的环境。
        backup = output.with_name("inference.previous")
        if backup.exists():
            shutil.rmtree(backup)
        if output.exists():
            output.rename(backup)
        try:
            stage.rename(output)
        except OSError:
            if backup.exists():
                backup.rename(output)
            raise
        if backup.exists():
            shutil.rmtree(backup)
    verify(output)
    print(f"推理环境已验证：{output}（{count} 个本地二进制文件）", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", type=Path, help="验证已打包的 inference 目录")
    args = parser.parse_args()
    try:
        if args.verify:
            audit_native(args.verify.resolve())
            verify(args.verify.resolve())
            print("独立推理验证通过")
        else:
            prepare(PROJECT / "desktop/src-tauri/target/bundled-runtime/inference")
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"准备失败：{error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        sys.exit(1)
