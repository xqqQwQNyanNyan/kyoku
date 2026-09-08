#!/usr/bin/env python3
"""把已验证的 Windows Mortal 与雀魂环境整理成可随应用移动的资源。"""

import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

PROJECT = Path(__file__).resolve().parent.parent
RUNTIME_COMMIT = "0cff2b52982be5b1163aa9a62fb01f03ce91e0d2"
MODEL_SHA = "738e0d6e3c0ce9671629554ad39abd147d2ffbac676e80b194c83f2acc0fea20"
PYTHON_VERSION = "3.12.10"
PYTHON_ARCHIVE = f"python-{PYTHON_VERSION}-embed-amd64.zip"
PYTHON_SHA = "4acbed6dd1c744b0376e3b1cf57ce906f9dc9e95e68824584c8099a63025a3c3"
NODE_VERSION = "22.23.2"
NODE_ARCHIVE = f"node-v{NODE_VERSION}-win-x64.zip"
NODE_SHA = "1177b4137ba5adaa56354ae40f1080c7450e8ae09cecb47da459d1c52ac99f97"
MAJSOUL_SOURCES = (
    "desktop.cjs",
    "desktop-session.cjs",
    "conversion.cjs",
    "client.cjs",
    "record.cjs",
    "package.json",
    "package-lock.json",
)


def run(*args, **kwargs):
    return subprocess.run(
        args, check=True, text=True, capture_output=True, **kwargs
    ).stdout.strip()


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def tree_digest(root):
    result = hashlib.sha256()
    for directory, dirs, files in os.walk(root):
        dirs[:] = sorted(name for name in dirs if name != "__pycache__")
        for name in sorted(files):
            if name.endswith((".pyc", ".pyo")):
                continue
            path = Path(directory) / name
            result.update(str(path.relative_to(root)).replace("\\", "/").encode())
            result.update(digest(path).encode())
    return result.hexdigest()


def ignored(_, names):
    return [
        name
        for name in names
        if name == "__pycache__" or name.endswith((".pyc", ".pyo"))
    ]


def download(url, target, expected_sha):
    if target.is_file() and digest(target) == expected_sha:
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".part")
    try:
        print(f"正在下载 {target.name}…", flush=True)
        with urllib.request.urlopen(url, timeout=60) as response, temporary.open("wb") as output:
            shutil.copyfileobj(response, output)
        if digest(temporary) != expected_sha:
            raise RuntimeError(f"{target.name} SHA-256 不匹配")
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def verify_pe_x64(path):
    with path.open("rb") as source:
        if source.read(2) != b"MZ":
            raise RuntimeError(f"不是 Windows PE 文件：{path}")
        source.seek(0x3C)
        header = struct.unpack("<I", source.read(4))[0]
        source.seek(header)
        if source.read(4) != b"PE\0\0" or struct.unpack("<H", source.read(2))[0] != 0x8664:
            raise RuntimeError(f"不是 Windows x64 文件：{path}")


def clean_windows_environment(root):
    names = ("SystemRoot", "WINDIR", "TEMP", "TMP", "USERPROFILE")
    environment = {name: os.environ[name] for name in names if name in os.environ}
    environment["PATH"] = str(root)
    return environment


def inference_events():
    return [
        {"type": "start_game", "names": ["A", "B", "C", "D"]},
        {
            "type": "start_kyoku",
            "bakaze": "E",
            "dora_marker": "1p",
            "kyoku": 1,
            "honba": 0,
            "kyotaku": 0,
            "oya": 0,
            "scores": [25000] * 4,
            "tehais": [
                ["1m", "2m", "3m", "4m", "5m", "6m", "7p", "8p", "9p", "E", "E", "P", "P"]
            ]
            + [["?"] * 13] * 3,
        },
        {"type": "tsumo", "actor": 0, "pai": "1s"},
    ]


def verify_inference(root):
    python = root / "python/python.exe"
    extension = root / "runtime/mortal/libriichi.pyd"
    for path in (python, extension):
        verify_pe_x64(path)
    if digest(root / "models/mortal_582500.pth") != MODEL_SHA:
        raise RuntimeError("打包权重 SHA-256 不匹配")
    pth = (root / "python/python312._pth").read_text(encoding="utf-8")
    if "Lib/site-packages" not in pth or "import site" not in pth:
        raise RuntimeError("嵌入式 Python 未启用私有 site-packages")
    input_text = "".join(json.dumps(event) + "\n" for event in inference_events())
    output = run(
        str(python),
        "-I",
        "-B",
        "-u",
        "-c",
        (PROJECT / "src/mortal/bridge.py").read_text(encoding="utf-8"),
        str(root / "runtime"),
        str(root / "models/mortal_582500.pth"),
        "0",
        input=input_text,
        env=clean_windows_environment(root / "python"),
        cwd=root,
        timeout=90,
    )
    lines = [json.loads(line) for line in output.splitlines()]
    if (
        len(lines) != 4
        or lines[0].get("sha256") != MODEL_SHA
        or lines[0].get("version") != 4
        or lines[-1].get("type") not in ("dahai", "reach")
        or not lines[-1].get("meta", {}).get("mask_bits")
    ):
        raise RuntimeError("打包环境未能完成真实切牌推理")


def verify_majsoul(root):
    node = root / "node/node.exe"
    verify_pe_x64(node)
    if any(path.name.startswith(".env") for path in root.rglob("*")):
        raise RuntimeError("下载组件中不允许包含 .env 文件")
    environment = clean_windows_environment(root / "node")
    if run(str(node), "--version", env=environment, cwd=root) != f"v{NODE_VERSION}":
        raise RuntimeError("Node 版本校验失败")
    response = run(
        str(node),
        str(root / "service/desktop.cjs"),
        input='{"action":"download","id":"test"}\n',
        env=environment,
        cwd=root,
        timeout=10,
    )
    if json.loads(response) != {"error": "login_required"}:
        raise RuntimeError("下载组件协议校验失败")
    program = r"""
const fs = require('node:fs');
const path = require('node:path');
const base = process.argv[1];
const sample = JSON.parse(fs.readFileSync(process.argv[2]));
const pb = require(path.join(base, 'node_modules/protobufjs'));
const root = pb.Root.fromJSON(require(path.join(base, 'node_modules/mjsoul/liqi.json')));
const data = sample.records.map(event => root.lookupType(event.name).fromObject(event.data));
const result = require(path.join(base, 'record.cjs')).convert({head:sample.head, data});
if (result.log.length !== 1 || result.name.length !== 4) process.exit(1);
process.stdout.write(JSON.stringify(result.log[0].at(-1)[1]));
"""
    result = run(
        str(node),
        "-e",
        program,
        str(root / "service"),
        str(PROJECT / "services/majsoul/test/fixtures/ranked-round.json"),
        env=environment,
        cwd=root,
        timeout=15,
    )
    if json.loads(result) != [-12000, 0, 13000, 0]:
        raise RuntimeError("打包转换器结算验证失败")


def replace_directory(stage, output):
    backup = output.with_name(output.name + ".previous")
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


def verify_cross_inference(root):
    for path in (
        root / "python/python.exe",
        root / "runtime/mortal/libriichi.pyd",
    ):
        verify_pe_x64(path)
    if digest(root / "models/mortal_582500.pth") != MODEL_SHA:
        raise RuntimeError("打包权重 SHA-256 不匹配")
    required = (
        root / "python/Lib/site-packages/torch/__init__.py",
        root / "python/Lib/site-packages/numpy/__init__.py",
        root / "runtime/mortal/model.py",
        root / "python/LICENSE.txt",
    )
    if not all(path.is_file() for path in required):
        raise RuntimeError("Windows 推理资源不完整")


def prepare_cross_inference(output, wheel_dir, extension):
    runtime = PROJECT / "mortal/runtime"
    model = PROJECT / "mortal/models/mortal_582500.pth"
    if run("git", "-C", str(runtime), "rev-parse", "HEAD") != RUNTIME_COMMIT:
        raise RuntimeError("Mortal 源码版本与固定版本不符")
    if run("git", "-C", str(runtime), "status", "--porcelain=v1", "--untracked-files=all"):
        raise RuntimeError("Mortal 源码存在本地修改")
    if digest(model) != MODEL_SHA:
        raise RuntimeError("Mortal 权重校验失败")
    verify_pe_x64(extension)
    wheels = sorted(wheel_dir.glob("*.whl"))
    names = [path.name.lower() for path in wheels]
    if not any(name.startswith("torch-2.14.0-") for name in names) or not any(
        name.startswith("numpy-2.5.2-") for name in names
    ):
        raise RuntimeError("缺少锁定版本的 Windows torch 或 numpy wheel")

    archive = output.parent / PYTHON_ARCHIVE
    download(
        f"https://www.python.org/ftp/python/{PYTHON_VERSION}/{PYTHON_ARCHIVE}",
        archive,
        PYTHON_SHA,
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="kyoku-inference-", dir=output.parent) as temporary:
        stage = Path(temporary) / "inference"
        python = stage / "python"
        python.mkdir(parents=True)
        with zipfile.ZipFile(archive) as source:
            source.extractall(python)
        packages = python / "Lib/site-packages"
        packages.mkdir(parents=True)
        for wheel in wheels:
            with zipfile.ZipFile(wheel) as source:
                source.extractall(packages)
        (python / "python312._pth").write_text(
            "python312.zip\n.\nLib/site-packages\nimport site\n", encoding="utf-8"
        )
        (stage / "runtime").mkdir()
        runtime_archive = Path(temporary) / "mortal.zip"
        subprocess.run(
            [
                "git",
                "-C",
                str(runtime),
                "archive",
                "--format=zip",
                f"--output={runtime_archive}",
                RUNTIME_COMMIT,
            ],
            check=True,
        )
        with zipfile.ZipFile(runtime_archive) as source:
            source.extractall(stage / "runtime")
        shutil.copy2(extension, stage / "runtime/mortal/libriichi.pyd")
        (stage / "models").mkdir()
        for name in (
            "mortal_582500.pth",
            "MODEL_CARD.md",
            "Mortal-LICENSE",
            "model-manifest.json",
        ):
            shutil.copy2(PROJECT / "mortal/models" / name, stage / "models" / name)
        shutil.copy2(PROJECT / "mortal/README.md", stage / "Mortal-SOURCES.md")
        verify_cross_inference(stage)
        inputs = {
            "prepare_script": digest(Path(__file__)),
            "bridge": digest(PROJECT / "src/mortal/bridge.py"),
            "python_archive": PYTHON_SHA,
            "extension": digest(extension),
            "runtime_commit": RUNTIME_COMMIT,
            "model_sha256": MODEL_SHA,
            "wheels": {wheel.name: digest(wheel) for wheel in wheels},
        }
        (stage / "manifest.json").write_text(
            json.dumps({"inputs": inputs, "cross_prepared": True}, indent=2) + "\n",
            encoding="utf-8",
        )
        replace_directory(stage, output)
    verify_cross_inference(output)
    print(f"Windows 推理资源已交叉准备（待 Windows 实机推理验收）：{output}")


def prepare_cross_majsoul(output):
    service = PROJECT / "services/majsoul"
    archive = output.parent / NODE_ARCHIVE
    download(f"https://nodejs.org/dist/v{NODE_VERSION}/{NODE_ARCHIVE}", archive, NODE_SHA)
    with tempfile.TemporaryDirectory(prefix="kyoku-majsoul-", dir=output.parent) as temporary:
        stage = Path(temporary) / "majsoul"
        (stage / "node").mkdir(parents=True)
        prefix = f"node-v{NODE_VERSION}-win-x64/"
        with zipfile.ZipFile(archive) as source:
            for source_name, target_name in (("node.exe", "node.exe"), ("LICENSE", "LICENSE")):
                with source.open(prefix + source_name) as data, (
                    stage / "node" / target_name
                ).open("wb") as target:
                    shutil.copyfileobj(data, target)
        (stage / "service").mkdir()
        for name in MAJSOUL_SOURCES:
            shutil.copy2(service / name, stage / "service" / name)
        shutil.copytree(service / "vendor", stage / "service/vendor")
        run(
            "npm",
            "ci",
            "--omit=dev",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            cwd=stage / "service",
            timeout=180,
        )
        verify_pe_x64(stage / "node/node.exe")
        if any(path.name.startswith(".env") for path in stage.rglob("*")):
            raise RuntimeError("下载组件中不允许包含 .env 文件")
        if not (stage / "service/node_modules/mjsoul/liqi.json").is_file():
            raise RuntimeError("Windows 雀魂下载组件依赖不完整")
        (stage / "manifest.json").write_text(
            json.dumps(
                {
                    "inputs": {
                        "prepare_script": digest(Path(__file__)),
                        "node_sha": NODE_SHA,
                        "package_lock": digest(service / "package-lock.json"),
                    },
                    "cross_prepared": True,
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        replace_directory(stage, output)
    print(f"Windows 雀魂资源已交叉准备（待 Windows 实机联网验收）：{output}")


def prepare_inference(output):
    if sys.version_info[:2] != (3, 12) or sys.prefix == sys.base_prefix:
        raise RuntimeError("请使用 build-windows.ps1 创建的 Python 3.12 虚拟环境")
    packages = Path(sys.prefix) / "Lib/site-packages"
    runtime = PROJECT / "mortal/runtime"
    model = PROJECT / "mortal/models/mortal_582500.pth"
    if run("git", "-C", str(runtime), "rev-parse", "HEAD") != RUNTIME_COMMIT:
        raise RuntimeError("Mortal 源码版本与固定版本不符")
    if run("git", "-C", str(runtime), "status", "--porcelain=v1", "--untracked-files=all"):
        raise RuntimeError("Mortal 源码存在本地修改")
    if digest(model) != MODEL_SHA:
        raise RuntimeError("Mortal 权重校验失败")
    versions = {name: importlib.metadata.version(name) for name in ("torch", "numpy")}
    if versions != {"torch": "2.14.0", "numpy": "2.5.2"}:
        raise RuntimeError("Python 依赖版本不匹配")
    extension_candidates = (
        runtime / "target/release/libriichi.dll",
        runtime / "target/release/riichi.dll",
    )
    extension = next((path for path in extension_candidates if path.is_file()), None)
    if extension is None:
        raise RuntimeError("未找到已编译的 libriichi.dll")
    verify_pe_x64(extension)

    cache = output.parent
    archive = cache / PYTHON_ARCHIVE
    download(
        f"https://www.python.org/ftp/python/{PYTHON_VERSION}/{PYTHON_ARCHIVE}",
        archive,
        PYTHON_SHA,
    )
    inputs = {
        "prepare_script": digest(Path(__file__)),
        "bridge": digest(PROJECT / "src/mortal/bridge.py"),
        "python_archive": PYTHON_SHA,
        "packages": tree_digest(packages),
        "extension": digest(extension),
        "runtime_commit": RUNTIME_COMMIT,
        "model_sha256": MODEL_SHA,
        **versions,
    }
    manifest = output / "manifest.json"
    if manifest.is_file() and json.loads(manifest.read_text(encoding="utf-8")).get("inputs") == inputs:
        verify_inference(output)
        print("Windows 推理环境已验证（复用）")
        return

    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="kyoku-inference-", dir=output.parent) as temporary:
        stage = Path(temporary) / "inference"
        python = stage / "python"
        python.mkdir(parents=True)
        with zipfile.ZipFile(archive) as source:
            source.extractall(python)
        shutil.copytree(packages, python / "Lib/site-packages", ignore=ignored)
        (python / "python312._pth").write_text(
            "python312.zip\n.\nLib/site-packages\nimport site\n", encoding="utf-8"
        )
        (stage / "runtime").mkdir()
        runtime_archive = Path(temporary) / "mortal.zip"
        subprocess.run(
            ["git", "-C", str(runtime), "archive", "--format=zip", f"--output={runtime_archive}", RUNTIME_COMMIT],
            check=True,
        )
        with zipfile.ZipFile(runtime_archive) as source:
            source.extractall(stage / "runtime")
        shutil.copy2(extension, stage / "runtime/mortal/libriichi.pyd")
        (stage / "models").mkdir()
        for name in ("mortal_582500.pth", "MODEL_CARD.md", "Mortal-LICENSE", "model-manifest.json"):
            shutil.copy2(PROJECT / "mortal/models" / name, stage / "models" / name)
        shutil.copy2(PROJECT / "mortal/README.md", stage / "Mortal-SOURCES.md")
        print("正在验证 Windows 独立环境中的模型推理…", flush=True)
        verify_inference(stage)
        (stage / "manifest.json").write_text(
            json.dumps({"inputs": inputs}, indent=2) + "\n", encoding="utf-8"
        )
        replace_directory(stage, output)
    verify_inference(output)
    print(f"Windows 推理环境已验证：{output}")


def prepare_majsoul(output):
    service = PROJECT / "services/majsoul"
    files = [service / name for name in MAJSOUL_SOURCES] + sorted((service / "vendor").rglob("*"))
    inputs = {str(path.relative_to(service)).replace("\\", "/"): digest(path) for path in files if path.is_file()}
    inputs.update({"node_sha": NODE_SHA, "prepare_script": digest(Path(__file__))})
    manifest = output / "manifest.json"
    if manifest.is_file() and json.loads(manifest.read_text(encoding="utf-8")).get("inputs") == inputs:
        verify_majsoul(output)
        print("Windows 雀魂组件已验证（复用）")
        return

    archive = output.parent / NODE_ARCHIVE
    download(f"https://nodejs.org/dist/v{NODE_VERSION}/{NODE_ARCHIVE}", archive, NODE_SHA)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="kyoku-majsoul-", dir=output.parent) as temporary:
        stage = Path(temporary) / "majsoul"
        (stage / "node").mkdir(parents=True)
        prefix = f"node-v{NODE_VERSION}-win-x64/"
        with zipfile.ZipFile(archive) as source:
            for source_name, target_name in (("node.exe", "node.exe"), ("LICENSE", "LICENSE")):
                with source.open(prefix + source_name) as data, (stage / "node" / target_name).open("wb") as target:
                    shutil.copyfileobj(data, target)
        (stage / "service").mkdir()
        for name in MAJSOUL_SOURCES:
            shutil.copy2(service / name, stage / "service" / name)
        shutil.copytree(service / "vendor", stage / "service/vendor")
        print("正在安装固定版本的雀魂下载器依赖…", flush=True)
        run(
            "npm.cmd",
            "ci",
            "--omit=dev",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            cwd=stage / "service",
            timeout=180,
        )
        verify_majsoul(stage)
        (stage / "manifest.json").write_text(
            json.dumps({"inputs": inputs}, indent=2) + "\n", encoding="utf-8"
        )
        replace_directory(stage, output)
    verify_majsoul(output)
    print(f"Windows 雀魂下载组件已验证：{output}")


def require_windows_x64():
    if sys.platform != "win32" or platform.machine().lower() not in ("amd64", "x86_64"):
        raise RuntimeError("Windows 资源必须在 Windows x64 环境准备和验证")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-inference", type=Path)
    parser.add_argument("--verify-majsoul", type=Path)
    parser.add_argument("--cross", action="store_true")
    parser.add_argument("--wheel-dir", type=Path)
    parser.add_argument("--extension", type=Path)
    args = parser.parse_args()
    try:
        root = PROJECT / "desktop/src-tauri/target/bundled-runtime/windows-x64"
        if args.cross:
            if args.wheel_dir is None or args.extension is None:
                parser.error("--cross 需要 --wheel-dir 和 --extension")
            prepare_cross_inference(
                root / "inference", args.wheel_dir.resolve(), args.extension.resolve()
            )
            prepare_cross_majsoul(root / "majsoul")
        else:
            require_windows_x64()
        if not args.cross and (args.verify_inference or args.verify_majsoul):
            if args.verify_inference:
                verify_inference(args.verify_inference.resolve())
            if args.verify_majsoul:
                verify_majsoul(args.verify_majsoul.resolve())
            print("Windows 独立资源验证通过")
        elif not args.cross:
            prepare_inference(root / "inference")
            prepare_majsoul(root / "majsoul")
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        print(f"准备失败：{error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        sys.exit(1)
