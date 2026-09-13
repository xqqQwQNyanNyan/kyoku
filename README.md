# Kyoku

## 项目简介

Kyoku 是一款日麻牌谱复盘桌面应用，支持导入天凤与雀魂牌谱、牌局回放、Mortal 本地分析，以及通过 LLM 辅助复盘。

## 怎么使用

在 release 中下载对应安装包，随后安装并启动 Kyoku，导入本地天凤牌谱或粘贴天凤／雀魂牌谱链接，然后选择复盘玩家。

你可以直接回放牌局，或运行 Mortal 分析查看决策点和推荐动作。需要 LLM 辅助时，先完成下方配置，再在“一起复盘”中提问。

## 配置 Endpoint / API Key

打开右上角“设置 → 连接”，填写：

- 服务地址：完整的 API Endpoint，以 `/responses` 或 `/chat/completions` 结尾。
- 模型名：服务商提供的模型名称，且模型需要支持工具调用。
- API Key：对应服务的密钥；本机无认证服务可留空。

先点击“测试连接”，成功后再点击“保存设置”。API Key 会以明文保存在本机应用配置文件中，请勿分享。

## 从源码构建

需要 Git、稳定版 Rust、Node.js 22.12+、Python 3.12，以及对应平台的 [Tauri 开发依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
git clone https://github.com/xqqQwQNyanNyan/kyoku.git
cd kyoku
```

Apple Silicon macOS：

```bash
npm --prefix desktop ci
bash scripts/setup-mortal.sh python3.12
bash scripts/build-macos.sh
```

Windows 11 x64（PowerShell）：

```powershell
.\scripts\build-windows.ps1
```

构建产物位于 `desktop/src-tauri/target/release/bundle/`；Windows 交叉目标位于 `desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/`。

## 实例演示

<video src="assets/Kyoku-demo-1x.mp4" controls width="100%"></video>

[无法播放时点击查看视频](assets/Kyoku-demo-1x.mp4)
