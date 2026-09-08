# Windows 发行方案

建议首版提供 Windows 11 x64 的完整 NSIS 安装包，CPU 推理，内置 Python、Mortal 权重和 Node。用户不需要安装开发环境。Windows 10 兼容性单独验收后再声明；首版不承诺 Windows ARM64 或 GPU 推理。本文件是实施方案，目前尚未产出、验证 Windows 安装包。

构建使用 Windows x64 机器或 GitHub Actions Windows runner。Tauri 支持 NSIS `-setup.exe` 和 MSI；官方将 macOS/Linux 交叉构建列为有额外限制的备用方案。本项目还包含 PyTorch 和 Python 原生扩展，因此优先在 Windows 原生构建、验证。[Tauri 安装包文档](https://v2.tauri.app/distribute/windows-installer/)

## 必要适配

| 位置 | 需要完成的工作 |
| --- | --- |
| `desktop/src-tauri/src/config.rs` | 按平台解析开发环境 `Scripts/python.exe` 和发行资源 `python/python.exe`，继续从应用资源目录定位，不依赖工作目录。 |
| `desktop/src-tauri/src/main.rs` | 引擎检查不能固定要求 `libriichi.so`；Windows 检查对应 `.pyd` 并实际加载模型。 |
| `src/mortal/mod.rs`、雀魂进程启动处 | Windows 子进程隐藏控制台，保留管道通信；检查退出、超时、停止后是否有残留进程。 |
| `desktop/src-tauri/src/majsoul/account.rs` | 发行资源使用 `node.exe`；清理环境变量时保留 Windows 运行所需变量，验证 HTTPS 和 Unicode 路径。 |
| Mortal 构建与资源准备 | 用同一 Python ABI 和 MSVC x64 编译 `libriichi.pyd`；收集 CPU PyTorch、NumPy 与依赖 DLL，验证模型实际推理。 |
| 设置、历史会话和导出 | Windows 实测覆盖保存、临时文件替换、重启恢复、中文及空格路径；发现平台差异时再做最小修复。 |

保留 Rust 主流程和现有 Python/Node 标准输入输出协议，平台差异集中在路径、进程和资源打包。

## 构建与资源

新增 `scripts/prepare-windows.py`、`scripts/build-windows.ps1` 和 `desktop/src-tauri/tauri.windows.conf.json`。Windows 资源放入独立目录，例如 `target/bundled-runtime/windows-x64/`，避免和 macOS 缓存混用。

Python 优先使用官方嵌入式发行版。构建阶段在匹配的完整 Python 环境准备依赖，再收集到发行目录，配置私有模块搜索路径；不让用户在嵌入式环境执行 pip。Python 官方明确建议将第三方包随应用一同分发。[Python 嵌入式发行文档](https://docs.python.org/3.12/using/windows.html#the-embeddable-package)

Mortal 沿用当前源码版本和权重校验值。PyTorch 选择官方 Windows CPU wheel，锁定通过实测的 Python、PyTorch、NumPy 组合；不直接复制 macOS 的二进制环境，也不预设相同版本一定有 Windows wheel。`libriichi` 使用通用 x64 编译目标，避免 `target-cpu=native` 把构建机指令集要求带给用户。[PyTorch Windows 安装说明](https://pytorch.org/get-started/locally/#installing-on-windows)

Node 使用官方 Windows x64 ZIP，校验 SHA-256；按现有白名单打包雀魂脚本、vendor 和生产依赖。模型、运行库的版本、哈希和许可记录一起进入资源清单。

NSIS 默认按当前用户安装。完整离线包使用 WebView2 `offlineInstaller`；同时检查并处理 Python、PyTorch 所需的 VC 运行库。后续需要缩小安装包时再提供联网引导版本。[Tauri WebView2 分发选项](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options)

CI 顺序：安装锁定工具链 → `npm ci` → 准备并验证运行资源 → Rust/前端/雀魂测试 → Tauri 构建 NSIS → 从安装目录运行验收 → 输出安装包和 SHA-256。预计入口如下，**所列脚本和配置尚待实现**：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1
# 脚本内部最终执行：
npm --prefix desktop run tauri -- build --target x86_64-pc-windows-msvc --config src-tauri/tauri.windows.conf.json
```

## 验收与交付

在没有 Python、Node、Rust 的干净 Windows 虚拟机上安装，从中文及空格路径启动。断网导入本地牌谱、回放并完成 Mortal 单点和整场分析；联网后验证 LLM 工具调用、多轮对话、停止、Token/费用/预算和历史导入导出。雀魂协议转换可以离线验收，真实登录下载另外使用获授权的测试账号验收。

最后验证覆盖安装、卸载和用户数据保留行为。内部测试包可以先不签名并明确标记；对外正式发行时给程序和安装器做代码签名。交付物为 `Kyoku_<version>_x64-setup.exe`、校验文件、版本说明和对应源码。具体包体积以首个成功构建为准。

实施顺序分三步：先在 Windows 跑通内置推理和 Node，再做安装包，最后接 CI 并完成干净机器验收。当前最大待验证点是 Python/PyTorch/`libriichi.pyd` 的完整依赖收集，而非界面迁移。
