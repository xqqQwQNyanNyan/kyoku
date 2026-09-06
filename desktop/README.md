# Kyoku Desktop

Tauri 2 + React + TypeScript 桌面复盘界面。Rust 核心仍位于仓库根目录；
桌面项目有独立的 Cargo 清单和锁文件，原有 CLI 无需编译 GUI 依赖。

## 启动

需要 Rust、Node.js 22.12+ 以及对应系统的 [Tauri 开发依赖](https://v2.tauri.app/start/prerequisites/)。
在本目录运行：

```bash
npm ci
npm run tauri dev
```

`npm run dev` 只启动前端开发服务器。文件解析、推理和问答需要 Tauri 桌面入口，
不能仅打开浏览器页面使用。

导入本地天凤 JSON（最大 16 MiB），选择复盘玩家，即可按事件播放、拖动进度或按局跳转。
左右方向键前后移动，空格播放或暂停；输入框中的按键不控制回放。
事件编号表示该事件应用后的局面，与现有 CLI 一致。

默认隐藏其他玩家手牌；勾选“显示全部手牌”可查看牌谱记录的四家暗牌。
摸切淡化显示，被鸣走的弃牌保留虚线占位。副露展示来源和鸣入牌，
加杠第一版按四张牌加文字标记展示，没有复刻实体牌的堆叠样式。

## 分析与问答

回放不依赖 Mortal 或 LLM 配置。点击“分析此玩家”后，后台运行整场 Mortal 推理；
期间可以继续回放。每份牌谱、每个玩家各缓存一次结果，完成后可按决策点跳转。
推荐动作独立展示，不用候选表中最大的 Q 值代替；Q 值不是概率或期望点数。
点击切牌候选可查看有效牌，原始证据中保留模型身份和完整计算结果。

Mortal 使用根 README 中准备的本地环境。开发版本默认以源码仓库根目录为资源目录，
也可在启动前设置绝对路径 `KYOKU_HOME`，目录内应包含：

```text
mortal/.venv/bin/python
mortal/runtime/
mortal/models/mortal_582500.pth
.env
```

问答时读取该目录中的 `.env`，环境变量优先；沿用 CLI 的 `OPENAI_MODEL`、
`KYOKU_OPENAI_ENDPOINT`、`AGENT_API_KEY` / `OPENAI_API_KEY` 约定。
自定义地址不会回退到官方密钥，密钥不会进入前端，也不会从 `.env` 注入 Mortal 子进程。
只有提问时才请求配置的 LLM 服务；导入和本地分析不发送牌谱到该服务。

问答仅支持选定玩家的决策点。切换事件或玩家后开始新会话，旧回答不进入新局面；
切回也不复用旧对话。显示全部手牌不会改变 Agent 的证据范围：
Agent 只收到当时自家暗牌、公开信息及对应分析，不收到对手暗牌或真实后续动作。
回答一次完整返回，当前尚未提供流式输出。失败时保留问题供重试。
切换局面不会取消已经发出的请求；它结束前暂不能发送新问题，但不影响牌谱浏览。

## 构建与检查

```bash
npm run build
npm test
npm run format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings

# macOS 本地调试应用，输出到 src-tauri/target/debug/bundle/macos/Kyoku.app。
npm run tauri -- build --debug --bundles app
```

Rust 回放测试使用与根项目相同的 `fixtures/tenhou/`，准备方式见根 README。
前端测试覆盖无模型回放、默认隐藏对手摸牌、最终推荐优先、旧回答隔离、旧分析隔离和失败重试。

当前是依赖本地运行环境的开发版本：安装包不包含 Python、PyTorch 和 Mortal 权重，
也没有配置签名、公证或自动更新。已在 macOS 验证；其他系统尚未做桌面验收，
Windows 的 Mortal 运行环境也仍需另行适配。
