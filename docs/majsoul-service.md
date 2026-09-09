# 雀魂下载组件

用户导入步骤统一见根 README。桌面版在本机运行下载组件，个人账号直接登录雀魂；无需部署服务。
已实测国际中文服（繁体中文）的账号密码登录及真实牌谱下载，其他服和第三方登录暂未接入。

## 桌面会话

`ImportDialog` 在识别到雀魂链接后显示登录区。首次登录必须确认风险，前端与 Rust 命令都检查同意状态。
`login_majsoul` 经 Rust 私有管道启动 `services/majsoul/desktop.cjs`，凭据不放入命令行、环境变量、
配置文件、日志或 Agent 会话。提交后清空前端密码；组件只保留已认证连接，不保存密码用于自动重登。
`majsoul_status` 仅返回是否有会话；`logout_majsoul` 结束组件进程。应用退出或崩溃导致管道关闭时，组件立即退出。

连接只使用固定的 `https://game.maj-soul.com/1` 资源入口，并从官方 HTTPS 响应发现协议和 WSS 网关。
不会读取独立服务的 `.env`、`ACCESS_TOKEN`、`MJS_GATEWAY` 或旧版 `KYOKU_MAJSOUL_ENDPOINT`。
Unity 连接先完成 `Route.requestConnection` 平台握手，再调用登录接口；资源版本变化时需更新 `client.cjs` 的已验证版本。
登录失败不自动重试，应用内登录尝试至少间隔 30 秒；验证码、短信和第三方授权需使用官方客户端。

下载响应只包含 Tenhou 格式牌谱，错误只返回白名单代码。stdout 专用于协议，第三方诊断输出丢弃。
桌面组件退出前通过管道区分依赖加载失败、运行异常、网关连接失败、连接关闭、心跳失败和服务器退出账号通知；
只有收到退出账号通知才提示检查其他客户端，不把所有进程退出都解释为账号冲突。
Rust 接收每份响应最多等待 45 秒；断线、损坏或过大的响应会结束会话。
不监听本地 HTTP 端口，不把用户账号交给共享服务。

## 转换与限制

`client.cjs` 解码新旧 Protobuf 容器，`record.cjs` 校验支持的普通四人段位规则、事件及子类型，
再调用 `vendor/tensoul` 转换器。未知类型必须明确拒绝，不应回退为猜测结果。
途中流局按 `RecordLiuJu.type` 的 1/2/3/4 映射为九种九牌、四风连打、四杠散了、四家立直。
本地修改与来源见 `vendor/tensoul/NOTICE.md`；升级 vendor 时核对回归测试。

`conversion.cjs` 由桌面会话与独立服务共用：同一账号同时只下载一份，结束后冷却 2 秒；
缓存最多 32 MiB、保留 1 小时，单份输出最多 16 MiB。缓存命中不请求雀魂，普通与匿名缓存分开。
缓存与会话一起销毁，不跨账号共享。下载间隔是本地保护措施，不代表雀魂官方认可的安全阈值。

东风场允许转换和回放，但当前 Mortal 引擎按半庄判断终盘；桌面禁用分析按钮，核心 Mortal 接口也拒绝东风场。
三麻、友人房、大会、活动规则暂不支持。真实样本尚未覆盖所有杠和多家和牌，不能据已有样本宣称完全兼容。

## 安装包

在准备好根 README 中的开发环境后运行：

```bash
bash scripts/build-macos.sh
```

`prepare-majsoul.py` 下载固定版本的官方 macOS arm64 Node.js 22.23.2，核验固定 SHA-256，
按 lockfile 安装依赖，只复制入口、转换代码和 vendor。不会复制 `.env` 或开发目录。
安装包资源布局为 `majsoul/node/bin/node` 和 `majsoul/service/desktop.cjs`；同时附带 Node 与依赖许可。
构建验证使用受限 PATH、工作目录 `/`，检查动态库依赖、入口通信、断开管道退出与实际单局转换。

```bash
mortal/.venv/bin/python scripts/prepare-majsoul.py
mortal/.venv/bin/python scripts/prepare-majsoul.py --verify \
  desktop/src-tauri/target/bundled-runtime/majsoul
```

更新运行时须同步版本和官方 SHA-256；不要直接复制依赖 Homebrew 动态库的系统 Node。

## 可选独立 HTTP 服务

保留 `server.cjs` 供维护者联调或独立工具使用，桌面端不依赖该入口。
以下命令从仓库根目录执行，已有 `.env` 时只编辑，不重复复制：

```bash
npm --prefix services/majsoul ci
cp services/majsoul/.env.example services/majsoul/.env
chmod 600 services/majsoul/.env
npm --prefix services/majsoul start
```

在 `.env` 中填写专用账号的 `MJS_USERNAME`、`MJS_PASSWORD`，默认 `MJS_BASE=https://game.maj-soul.com/1`。
可选 `ACCESS_TOKEN` 非空时使用 `oauth2Login`，类型由 `MJS_LOGIN_TYPE` 指定（默认 10，此路径尚未实测）。
Unity 资源版本可用 `MJS_RESOURCE_VERSION` 覆盖；自定义 `MJS_GATEWAY` 时还需匹配的 `MJS_ROUTE_ID`。
`HOST`、`PORT` 默认 `127.0.0.1:2563`。凭据失效后编辑文件并重启，不把文件打包或提交。

```bash
curl --fail --get http://127.0.0.1:2563/convert \
  --data-urlencode 'id=<分享链接 paipu= 后的完整值>' -o /tmp/kyoku-majsoul.json
cargo run --bin replay -- /tmp/kyoku-majsoul.json
```

`GET /healthz` 只检查 HTTP 存活。`GET /convert?id=...` 成功返回 Tenhou JSON，失败返回
`{"error":{"code":"..."}}`：400 编号无效、413 过大、422 不支持、429 冷却、502 上游失败、503 忙碌或连接不可用、504 超时。
worker 通过独立进程隔离协议异常，失败或超时后下次请求重建。该服务使用运营账号，不接受用户上传账号密码。

部署时保持回环监听，由反向代理提供 HTTPS、至少 60 秒响应超时和按 IP 限流；避免记录带牌谱编号的查询参数。
当前仅适用于小规模联调，公开服务还需单独验证账号规则、预期负载和跨服访问。

## 验证

```bash
npm --prefix services/majsoul test
cargo test
cargo test --manifest-path desktop/src-tauri/Cargo.toml
npm --prefix desktop test
```

自动测试使用本地样本和模拟连接，不使用真实凭据。覆盖流局类型、未知类型拒绝、新旧 Protobuf、
缓存限流、登录同意与凭据校验、错误脱敏、会话复用退出、进程超时、UI 登录失败和东风场分析限制。
真实联调只使用已授权的小号与可访问的牌谱，不把凭据或原始登录响应保存为 fixture。

Windows 安装包的无账号诊断入口和操作见根 README。`scripts/diagnose-majsoul.cjs`
复用已安装的 client，使用与桌面相同的环境变量白名单及持有 stdin 的子进程检查管道，
默认只放行 `Route.requestConnection`，在账号登录前停止，不能把探测成功当作真实登录验收。
显式 `-Login` 模式用隐藏控制台和同一环境白名单启动 Node，通过 UTF-8 私有管道传入凭据；
额外放行一次 `Lobby.login`、一次 `Lobby.loginSuccess` 和心跳，记录数秒连接状态后退出。
报告只输出阶段、白名单错误类型、错误码和 HTTP／关闭状态码，不输出请求、响应、账号或 token；不自动重试登录。
PowerShell 入口保留 UTF-8 BOM，以兼容 Windows PowerShell 5.1；凭据通过无 BOM 的 UTF-8 文本写入器送入子进程。
可运行 `powershell -NoProfile -File scripts/test-diagnose-majsoul.ps1` 验证启动脚本；
测试使用假子进程检查中文和引号密码的管道传输、退出状态及脱敏报告，不调用雀魂。
`-Replay` 通过安装包原始 `desktop.cjs` 的私有管道执行登录和下载；`trace-majsoul.cjs` 作为诊断预加载脚本，
在 stderr 记录白名单事件，保留原 stdout 响应和入口行为。父进程校验响应格式与大小，只报告结果，不保存牌谱正文。
