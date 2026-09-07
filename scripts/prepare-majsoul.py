#!/usr/bin/env python3
"""为 macOS 安装包准备独立雀魂下载组件，不复制 .env 或用户凭据。"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

PROJECT = Path(__file__).resolve().parent.parent
NODE_VERSION = "22.23.2"
NODE_ARCHIVE = f"node-v{NODE_VERSION}-darwin-arm64.tar.gz"
NODE_SHA = "61130f394c1630d211dd50aecc4353d379480f36d3ac913cd85dbba1aed585c6"
SOURCES = ("desktop.cjs", "desktop-session.cjs", "conversion.cjs", "client.cjs", "record.cjs", "package.json", "package-lock.json")


def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs).stdout.strip()


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def verify(root):
    node = root / "node/bin/node"
    # 官方 Node 发行包不应依赖构建机上的 Homebrew；安装后不需要系统 Node。
    for line in run("otool", "-L", str(node)).splitlines()[1:]:
        dependency = line.strip().split(" (compatibility")[0]
        if not dependency.startswith(("/usr/lib/", "/System/Library/")):
            raise RuntimeError(f"Node 依赖包外动态库：{dependency}")
    if run("lipo", "-archs", str(node)) != "arm64":
        raise RuntimeError("Node 架构不是 arm64")
    if any(path.name.startswith(".env") for path in root.rglob("*")):
        raise RuntimeError("下载组件中不允许包含 .env 文件")
    environment = {"PATH": "/usr/bin:/bin"}
    if run(str(node), "--version", env=environment, cwd="/") != f"v{NODE_VERSION}":
        raise RuntimeError("Node 版本校验失败")
    # 验证实际入口和未登录拦截，关闭管道后进程必须退出。
    with subprocess.Popen([str(node), str(root / "service/desktop.cjs")],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, env=environment, cwd="/") as process:
        try:
            process.stdin.write('{"action":"download","id":"test"}\n')
            process.stdin.flush()
            import select
            if not select.select([process.stdout], [], [], 10)[0]:
                raise RuntimeError("下载组件没有响应")
            if json.loads(process.stdout.readline()) != {"error": "login_required"}:
                raise RuntimeError("下载组件协议校验失败")
            process.stdin.close()
            process.wait(timeout=10)
            if process.returncode != 0:
                raise RuntimeError("下载组件退出异常")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    # 同时实际加载打包的 Protobuf 和转换器，防止只打包入口而漏掉依赖。
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
    result = run(str(node), "-e", program, str(root / "service"),
                 str(PROJECT / "services/majsoul/test/fixtures/ranked-round.json"),
                 env=environment, cwd="/", timeout=15)
    if json.loads(result) != [-12000, 0, 13000, 0]:
        raise RuntimeError("打包转换器结算验证失败")


def prepare(output):
    if sys.platform != "darwin" or platform.machine() != "arm64":
        raise RuntimeError("目前仅支持构建 Apple Silicon macOS 下载组件")
    service = PROJECT / "services/majsoul"
    files = [service / name for name in SOURCES] + sorted((service / "vendor").rglob("*"))
    inputs = {str(path.relative_to(service)): digest(path) for path in files if path.is_file()}
    inputs.update({"node_sha": NODE_SHA, "prepare_script": digest(Path(__file__))})
    manifest = output / "manifest.json"
    if manifest.is_file() and json.loads(manifest.read_text()).get("inputs") == inputs:
        verify(output)
        print("雀魂下载组件已验证（复用）")
        return
    output.parent.mkdir(parents=True, exist_ok=True)
    archive = output.parent / NODE_ARCHIVE
    if not archive.is_file() or digest(archive) != NODE_SHA:
        print(f"正在下载官方 Node.js {NODE_VERSION}…", flush=True)
        temporary = archive.with_suffix(".part")
        try:
            with urllib.request.urlopen(f"https://nodejs.org/dist/v{NODE_VERSION}/{NODE_ARCHIVE}", timeout=30) as response, temporary.open("wb") as target:
                shutil.copyfileobj(response, target)
            if digest(temporary) != NODE_SHA:
                raise RuntimeError("Node.js 下载文件 SHA-256 不匹配")
            temporary.replace(archive)
        finally:
            temporary.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="kyoku-majsoul-", dir=output.parent) as temporary:
        stage = Path(temporary) / "majsoul"
        (stage / "node/bin").mkdir(parents=True)
        with tarfile.open(archive) as source:
            for member, destination in (("bin/node", "bin/node"), ("LICENSE", "LICENSE")):
                with source.extractfile(f"node-v{NODE_VERSION}-darwin-arm64/{member}") as data:
                    (stage / "node" / destination).write_bytes(data.read())
        os.chmod(stage / "node/bin/node", 0o755)
        (stage / "service").mkdir()
        for name in SOURCES:
            shutil.copy2(service / name, stage / "service" / name)
        shutil.copytree(service / "vendor", stage / "service/vendor")
        print("正在安装固定版本的下载器依赖…", flush=True)
        run("npm", "ci", "--omit=dev", "--ignore-scripts", "--no-audit", "--no-fund", cwd=stage / "service", timeout=180)
        run("codesign", "--force", "--sign", "-", str(stage / "node/bin/node"))
        verify(stage)
        (stage / "manifest.json").write_text(json.dumps({"inputs": inputs}, indent=2) + "\n")
        backup = output.with_name("majsoul.previous")
        if backup.exists(): shutil.rmtree(backup)
        if output.exists(): output.rename(backup)
        try:
            stage.rename(output)
        except OSError:
            if backup.exists(): backup.rename(output)
            raise
        if backup.exists(): shutil.rmtree(backup)
    print("雀魂下载组件已验证，安装后无需配置服务或安装 Node.js")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", type=Path)
    args = parser.parse_args()
    try:
        if args.verify:
            verify(args.verify.resolve())
            print("独立雀魂下载组件验证通过")
        else:
            prepare(PROJECT / "desktop/src-tauri/target/bundled-runtime/majsoul")
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"下载组件准备失败：{error}", file=sys.stderr)
        sys.exit(1)
