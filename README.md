# Kyoku

一个用 Rust 编写的日麻牌谱复盘助手。

目前提供桌面 GUI 和命令行。macOS 桌面版可构建包含完整本地推理环境的安装包。

## macOS 安装包使用

安装包支持 **Apple Silicon（M 系列芯片）、macOS 14 及以上**，暂不提供 Intel Mac 和 Windows 包。
打开 `Kyoku_0.1.0_aarch64.dmg`，将 Kyoku 拖入“应用程序”后启动。
包内包含 Python、PyTorch、NumPy、Mortal 引擎和默认权重，无需安装开发工具或首次联网下载分析组件。

点击“导入牌谱”，选择本地文件、天凤链接或内置示例牌谱，然后选择玩家并点击“分析此玩家”。
本地回放和分析无需账号或 API Key；只有链接下载和 Agent 问答需要联网。

需要问答时，打开右上角“设置”，填写完整 Responses 或 Chat Completions 服务地址、模型名和对应 API Key。
可点击“测试连接”检查认证、模型和工具调用能力，再点击“保存设置”。测试会发送一次不含牌谱的请求，
可能产生少量调用费用；测试成功不会自动保存。保存新配置会清空当前问答历史。
“检查引擎”会在本地加载内置模型，确认运行环境可用。

API Key 保存在 macOS 钥匙串，不回显到界面。相同地址下留空保留现有密钥；更换地址后必须重新填写，
本机无认证服务可以留空。勾选“删除已保存的密钥”并保存可移除密钥。
服务地址和模型名保存在 `~/Library/Application Support/dev.kyoku.desktop/settings.json`，升级应用不会覆盖它们。
安装包不读取源码目录、`.env` 或终端中的问答环境变量。

当前本地构建使用 ad-hoc 签名，**尚无 Developer ID 签名和 Apple 公证**，不等同于可直接公开发行的已公证安装包。
其他电脑的首次打开可能受到 Gatekeeper 限制；公开发行前需完成签名与公证。

## 从源码运行

下面先跑通示例牌谱回放，再启用本地分析和问答。除特别说明外，命令都在仓库根目录运行。

### 1. 准备环境并获取项目

| 用途 | 需要安装 |
| --- | --- |
| 命令行回放 | 稳定版 Rust 工具链、Git |
| 桌面 GUI | 上述工具、Node.js 22.12+、对应系统的 [Tauri 开发依赖](https://v2.tauri.app/start/prerequisites/) |
| Mortal 分析 | Python 3.11+（推荐 3.12）、curl；安装时需联网下载依赖和约 125 MiB 的权重 |
| Agent 问答 | 支持 Responses 或 Chat Completions 工具调用的 LLM 服务、模型名及对应密钥 |

桌面版已在 macOS 验证。Mortal 安装脚本支持 macOS 和 Linux；Linux 桌面尚未完成验收，
Windows 的 Mortal 环境仍需适配。下述步骤用于开发；安装包用户无需执行。

```bash
git clone https://github.com/xqqQwQNyanNyan/kyoku.git
cd kyoku
cargo run --bin replay -- fixtures/tenhou/ranked_game.json
```

看到逐事件局面变化和结束摘要，说明牌谱解析与回放正常。`fixtures/tenhou/` 已随仓库提供，
不需要另行下载示例牌谱，也不需要 Mortal 或 LLM 配置。

### 2. 启动桌面回放

```bash
cd desktop
npm ci
npm run tauri dev
```

顶栏和主页的“导入牌谱”都会打开来源选择：
选择“本地文件”可打开天凤 JSON；选择“天凤链接”可粘贴链接或 log ID，再点击“导入链接”（需要联网）；
选择“示例牌谱”可从 21 份内置样本中选择，再点击“导入示例”（无需联网）。也可以直接拖入本地 JSON 文件。
用底部播放按钮、进度条或局数选择浏览牌局，左右方向键移动一个事件，空格播放或暂停。
默认隐藏其他玩家手牌；需要查看牌谱中的全部手牌时，勾选“显示全部手牌”。

`npm run dev` 只启动前端服务器；使用完整功能请运行 `npm run tauri dev`。
关闭窗口并在终端按 Ctrl+C 可结束开发服务。下面的准备步骤请在仓库根目录执行；
如果仍在 `desktop/`，先运行 `cd ..`，也可以另开终端进入仓库根目录。

### 3. 启用 Mortal 分析

```bash
bash scripts/setup-mortal.sh python3.12
cargo run --bin review -- --player 0 --event 2 fixtures/tenhou/ranked_game.json
```

将 `python3.12` 替换为本机 Python 3.11+ 的命令或可执行文件绝对路径。
脚本创建 Python 虚拟环境、下载固定版本的 Mortal 源码、编译 `libriichi`，并下载和校验权重。
首次安装需要一些时间，末尾出现 `Mortal checkpoint verified` 和 `Ready` 表示准备完成。
随后 `review` 应显示局面、Mortal 推荐与候选切牌的向听、进张。

回到 GUI，选择玩家并点击“分析此玩家”。后台完成后，用“上一决策”或“下一决策”
跳转到可分析的局面；右侧展示最终推荐和候选 Q 值，点击切牌候选查看有效牌。
这一步仍不需要 LLM 配置。

### 4. 配置问答并提出第一个问题

GUI 推荐直接在“设置”中配置。下面的 `.env` 方式用于 CLI，以及尚未保存界面设置的开发版 GUI。

第一次配置时，在仓库根目录复制示例；已有 `.env` 时直接编辑它，避免覆盖自己的配置。

```bash
cp .env.example .env
```

使用中转站或其他兼容服务时，填写完整的 API 地址、模型名和专用密钥：

```dotenv
KYOKU_OPENAI_ENDPOINT='https://your-service.example/v1/responses'
OPENAI_MODEL='your-model-name'
AGENT_API_KEY='your-key'
```

使用官方服务时，删除或注释 `.env` 中的 `KYOKU_OPENAI_ENDPOINT` 和 `AGENT_API_KEY`，改为：

```dotenv
OPENAI_MODEL='your-model-name'
OPENAI_API_KEY='your-key'
```

服务提供 Chat Completions 时，把地址改为 `https://your-service.example/v1/chat/completions`。
GUI 设置页也可直接填写该地址，程序按地址自动选择协议，不需要额外切换选项。
标准路径是复数 `/chat/completions`；若供应商明确提供单数 `/chat/completion`，也按 Chat 协议处理，保留原地址。

上面的地址、模型名和密钥都是占位符，须替换成服务实际支持的值。两种协议都要求服务和模型
支持 `tools`、`tool_choice` 及工具结果回传；只有普通聊天能力的服务仍不能使用。
密钥用单引号包裹，避免 `$` 被展开；
`.env` 已被 Git 忽略，不要把真实密钥写进命令或提交到仓库。

在 GUI 中跳到一个决策点，输入“比较这里的候选切牌，说明向听、进张和 Mortal 的倾向。”
并发送；也可以用命令行验证：

```bash
cargo run --bin agent -- --player 0 --event 2 \
  --question '比较这里的候选切牌，说明向听、进张和 Mortal 的倾向。' \
  fixtures/tenhou/ranked_game.json
```

得到解释后，就跑通了“牌谱 → 局面重建 → 分析 → Mortal → Agent”的流程。
同一局面可以继续追问；切换事件或玩家会清空问答历史，切回也不会恢复旧对话。

## 使用自己的牌谱

GUI 接受天凤牌谱链接、log ID，以及本地四人天凤 JSON（`tenhou.net/6` 格式，最大 16 MiB）；命令行示例中的
`fixtures/tenhou/ranked_game.json` 也可以直接替换为自己的 JSON 路径。带空格的路径用引号包裹。
当前不能直接导入雀魂链接、雀魂原始牌谱、天凤 XML 或 mjai JSONL。

如果手上只有天凤牌谱链接，可以在 GUI 中点击“导入牌谱”并选择“天凤链接”，也可以通过 `replay` 直接回放：

```bash
cargo run --bin replay -- 'https://tenhou.net/0/?log=<log-id>&tw=0'
cargo run --bin replay -- '<log-id>'
```

将 `<log-id>` 替换成实际牌谱编号。GUI 下载成功后即可回放和分析，不需要手动保存文件。
下载最多等待 30 秒，牌谱上限同样为 16 MiB；失败时保留当前牌谱和输入，便于重试。
链接中的 `tw` 不会自动切换复盘玩家，请在导入后选择玩家。

`mortal`、`review` 和 `agent` 仍需要本地 JSON；可以先下载同一份牌谱，再分析：

```bash
curl -fL --referer 'https://tenhou.net/' \
  'https://tenhou.net/5/mjlog2json.cgi?<log-id>' -o /tmp/kyoku-game.json
cargo run --bin review -- --player 0 /tmp/kyoku-game.json
```

下载受天凤服务可用性和牌谱访问权限影响；初次体验可以先使用仓库示例。
`replay` 的终端输出是可读的局面摘要，不能重定向后当作 JSON 导入 GUI。

`--player 0..3` 是牌谱中整场不变的玩家索引，对应 JSON 的 `name` 数组顺序，
不是当前东南西北座位。所有入口使用相同的零基全局事件编号；`--event N`
表示第 N 个事件应用后的局面，不是第 N 巡。可先用 `replay` 或整场决策列表找到编号。

仓库内 21 份样本覆盖双响、流局、抢杠、岭上摸牌、连续杠和复杂鸣牌；部分仅包含几局。
来源、固定版本及文件对应关系见[示例牌谱来源](fixtures/tenhou/README.md)。

## 桌面使用

分析在后台运行，期间可以继续回放。每份牌谱、每个玩家的整场分析缓存在当前进程中，
切换决策不需要重新推理；关闭应用后缓存消失。导入新牌谱后需要重新分析。

问答只支持选定玩家的决策点。提示没有行动机会时，用“下一决策”移动到可提问的局面。
回答目前一次完整返回；切换局面不会取消已经发出的请求，请等它结束后再发送新问题。
失败时保留问题，便于重试。原始证据可用于核对解释中的具体数字。

窗口调整大小时，界面等比例缩放，牌桌和底部回放控制始终完整显示，无需滚动页面。
右侧用「决策分析」和「一起复盘」标签切换显示；切换标签保留分析、对话和未发送的问题。
只有聊天记录和 Mortal 判断内容可以滚动；牌局较多时使用列表下方的翻页按钮。
手牌、副露和牌河使用固定的统一牌尺寸，只随窗口缩放；点况位于深色牌河区域四角。
牌河中的摸切用浅棕牌底表示，被鸣走的牌淡化保留原位；悬停可查看完整标记。
输入框中的方向键和空格不会触发回放快捷键。

源码开发版本默认从仓库根目录读取 Mortal；尚未保存界面设置时读取 `.env`。需要改用其他资源目录时，
在启动前设置绝对路径：

```bash
# 在仓库根目录运行；也可以换成已准备好资源的其他绝对路径。
export KYOKU_HOME="$PWD"
cd desktop
npm run tauri dev
```

该目录的结构应为：

```text
mortal/.venv/bin/python
mortal/runtime/
mortal/models/mortal_582500.pth
.env                              # 仅问答需要
```

`KYOKU_HOME` 只用于桌面版。CLI 默认路径相对于当前工作目录，因此请在仓库根目录运行，
或使用下面的路径覆盖参数。Python 虚拟环境包含绝对路径，搬动项目后需要重新创建。

## 命令行使用

### 回放与定位局面

```bash
cargo run --bin replay -- --full-state fixtures/tenhou/rinshan.json
cargo run --bin replay -- --kyoku E2 --only hora,kan,dora,ryukyoku fixtures/tenhou/ranked_game.json
cargo run --bin replay -- --from 120 --to 140 fixtures/tenhou/ranked_game.json
cargo run --bin replay -- --state-at 120 fixtures/tenhou/ranked_game.json
```

`--event N` 只显示指定事件，`--from` 和 `--to` 的范围包含两端；`--state-at N`
额外输出该事件后的完整局面，`--full-state` 输出最终完整局面。
`--kyoku E2` 包含东二局的所有本场，`E2.1` 只选一本场，南场使用 `S`。
`--only` 只过滤显示，所有事件仍会进入回放器，局与比赛的结束摘要也会保留。
回放失败时会报告事件编号、附近事件和最后一个有效局面。

### Mortal 判断与综合复盘

```bash
# 只查看 Mortal 判断。
cargo run --bin mortal -- --player 0 --event 2 fixtures/tenhou/ranked_game.json

# 同时查看可见局面、切牌效率与 Mortal 判断。
cargo run --bin review -- --player 1 --event 30 fixtures/tenhou/complex_nakis.json

# 省略 --event：mortal 输出整场判断，review 列出整场行动机会。
cargo run --bin review -- --player 0 fixtures/tenhou/ranked_game.json
```

`--player` 必填。单局面查询每次加载模型；整场查询只加载一次，沿真实牌谱推进，
不会自动执行模型建议。没有行动机会的单局面仍可展示局面；吃、碰、和牌和跳过保留模型判断，
只有切牌候选附带牌效率。

整场列表显示局数、本场、自家手番、实际动作和最终推荐。手番按自家牌河长度加一计算，
鸣牌响应标在下一次出牌手番，杠不单独增加手番。无法确认的实际选择标为“无法确定”，
不当作跳过。当前不进行失误评分或排序。

`mortal`、`review` 和 `agent` 都支持 `--python PATH`、`--runtime PATH`、`--model PATH`，
分别覆盖 Python、Mortal 源码目录和权重文件；默认路径与上面的目录结构相同。

### 交互问答与整场浏览

```bash
# 不运行 Mortal，只询问可见局面；缺少的分析会明确说明。
cargo run --bin agent -- --player 0 --event 2 --without-mortal fixtures/tenhou/ranked_game.json

# 同一局面反复提问；输入 /quit 退出。
cargo run --bin agent -- --player 0 --event 2 fixtures/tenhou/ranked_game.json

# 缓存整场决策，然后选择局面并提问。
cargo run --bin agent -- --player 0 --browse fixtures/tenhou/ranked_game.json
```

| 交互命令 | 用途 |
| --- | --- |
| `/evidence` | 查看当前局面的原始 JSON 证据 |
| `/quit` | 退出 |
| `/list` | 重看整场决策列表，仅浏览模式 |
| `/select N` | 按全局事件编号选择决策，仅浏览模式 |
| `/next`、`/prev` | 切换相邻决策，仅浏览模式 |
| `/show` | 查看当前局面、牌效率和候选，仅浏览模式 |

直接输入文字即可提问。浏览模式最初选择第一个决策点，无效选择保留当前局面和问答。
浏览、`/show` 和 `/evidence` 不需要 LLM 配置；默认启动仍需 Mortal，且已有 `.env` 的语法须正确。
单局面加 `--without-mortal` 可跳过 Mortal，不需要 Python 或权重；只提供可见局面，
切牌效率和 Mortal 标为未分析。该选项不能与 `--browse` 同用。
`--browse` 只接受文件，不能与 `--event` 或 `--question` 同用。

单局面可用 `--question '问题'` 回答一次后退出；加上 `--interactive` 则回答后继续交互。
所有 CLI 都支持 `-` 从标准输入读取牌谱，但 `agent` 此时必须使用单次 `--question`，
不能加 `--interactive` 或使用浏览模式。例如：

```bash
cat fixtures/tenhou/ranked_game.json | cargo run --bin agent -- \
  --player 0 --event 2 --question '这里有哪些有效牌？' -
```

`agent` 自动读取当前工作目录的 `.env`，配置优先级为命令行参数、已有环境变量、`.env`。
`--llm-model NAME` 覆盖问答模型名，`--endpoint URL` 覆盖完整 Responses 或 Chat Completions 地址；
`--model` 始终指 Mortal 权重，不是 LLM 模型。
GUI 优先使用设置页保存的配置。仅开发版在没有已保存设置时，才依次读取已有环境变量和资源目录内的 `.env`。

默认官方地址只读取 `OPENAI_API_KEY`。一旦显式覆盖地址，就只读取独立的 `AGENT_API_KEY`，
不会回退到官方密钥；远程自定义服务必须设置专用密钥，本地无认证服务可省略它。
每个 CLI 的完整参数可用 `cargo run --bin <名称> -- --help` 查看，名称为
`replay`、`mortal`、`review` 或 `agent`。

## 如何理解结果

Mortal 的“最终推荐”可能与候选表中 Q 值最高的动作不同，因为引擎还会应用和牌规则保护。
两者冲突时以最终推荐为准；CLI 中对应 `Mortal:` 行。Q 值是原始模型输出，
不是概率或期望点数。杠牌种选择有独立的第二层评价，不能与主动作 Q 值混排。

默认使用 Mortal V4 的 CPU 推理及
[Yuchen1457/mortal-582500 社区四麻权重](https://huggingface.co/Yuchen1457/mortal-582500)，
并非 Mortal 官网的官方权重，不据此声称相同棋力。CLI 输出实际加载权重的 SHA-256。
源码版本、模型来源及许可附件记录在 [Mortal 来源说明](mortal/README.md)。

牌效率中的不可见枚数不是实际牌山剩余枚数，完成牌形也不代表可以合法和牌。
Agent 按问题使用【局面】【计算】【Mortal】【说明】段落，程序统一排版并校验引用的证据路径和原始值；
格式或引用不合格时最多自动纠正两次，仍失败则报错，不展示不合格回答。未运行 Mortal 时明确显示“未分析”。
正文 Q 值保留三位小数，字牌使用东、南、西、北、白、发、中；原始证据仍保留完整精度和牌值编码。
有效牌明细按花色分行、相同不可见枚数合并，由程序从计算结果生成，并列出牌种数和不可见总枚数。
当前向听和直接进张相同不代表整体牌效率相同；缺少后续改良、打点或防守分析时，
Agent 应承认证据不足，不强行为模型偏好编造理由。正文推理仍不能逐句自动验证，可用原始证据核对。

回放和 Mortal 分析在本地进行。提问时，LLM 服务收到问题、自家暗牌、公开信息、
牌效率和 Mortal 判断，不会收到完整牌谱、对手暗牌、实际后续动作或其他局面的对话。
“显示全部手牌”不会扩大 Agent 的证据范围。会话保存在当前进程内存中，
请求使用 `store: false`；这不等于服务商承诺零数据留存。

当前支持四人牌谱的回放、局面分析和问答；尚不提供整场自动找错、顺位预测、押退风险计算，
macOS 安装包已包含 Python 和模型；尚未提供 Developer ID 签名、Apple 公证和自动更新。

## 常见问题

| 现象 | 处理方式 |
| --- | --- |
| 找不到牌谱或 `.env` | 确认 CLI 在仓库根目录运行，或为牌谱使用绝对路径；示例文件已随仓库提供。 |
| 无法启动 Mortal、找不到 Python 或 `libriichi` | 先完成安装脚本，再运行快速开始中的 `review` 命令；GUI 还需检查 `KYOKU_HOME` 是否指向资源目录。 |
| 安装脚本报告 Mortal 版本或工作区不符 | 检查 `mortal/runtime` 中的本地改动，保存自己的工作后恢复到脚本指定的干净版本再重试；脚本不会替你覆盖改动。 |
| 配置后仍提示模型或认证错误 | 检查是否还留着示例占位符、是否填写服务的完整 `/responses` 或 `/chat/completions` 地址、服务是否支持工具调用，以及已有环境变量是否覆盖了 `.env`。 |
| GUI 无法提问 | 先分析选定玩家，再用“下一决策”进入决策点；已有请求进行中时等待其结束。 |

## 开发

代码按 `replay → mahjong / analysis → Mortal / Agent → GUI` 分层；
领域层维护规则和状态，上层组合分析结果。用户用法集中维护在本 README，
本地 `docs/` 用于开发设计与验证记录，不作为使用前提。

根项目的检查不需要 Python、权重或在线 LLM，回放测试直接使用仓库中的示例牌谱：

```bash
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --all-features -- -D warnings
bash scripts/test-setup-mortal.sh
```

桌面项目有独立的依赖和锁文件。在仓库根目录运行：

```bash
npm --prefix desktop ci
npm --prefix desktop run build
npm --prefix desktop test
npm --prefix desktop run format:check
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check
cargo test --manifest-path desktop/src-tauri/Cargo.toml --locked
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings

# macOS 本地调试应用，仍依赖上文准备的本地资源。
npm --prefix desktop run tauri -- build --debug --bundles app
```

调试应用输出到 `desktop/src-tauri/target/debug/bundle/macos/Kyoku.app`。

### 构建完整 macOS 安装包

在 Apple Silicon Mac 上先完成上述源码环境准备及 `setup-mortal.sh`，使用 Python 3.12。
打包要求 Python 基础发行版本身可移动，例如 [python-build-standalone](https://github.com/astral-sh/python-build-standalone)
的 `aarch64-apple-darwin` install-only 发行版；普通 Homebrew Python 可能依赖包外动态库，不能直接复制分发。
如果现有虚拟环境基于不可移动的 Python，先保留自己的环境，再用独立发行版重新创建 `mortal/.venv` 并运行准备脚本。

```bash
bash scripts/build-macos.sh
```

脚本从当前 Mortal 虚拟环境整理独立的 Python 运行目录，保留依赖许可、上游源码和模型来源附件，
校验固定版本及权重 SHA-256，检查动态库依赖，并实际运行一次模型推理。
不复制 `.env`、用户设置、额外权重或 Rust 构建缓存。产物只放在已忽略的 `desktop/src-tauri/target/` 中。
源环境内容不变时复用已整理的资源；无需每次重新安装 Python 或 PyTorch。

输出位置：

```text
desktop/src-tauri/target/release/bundle/macos/Kyoku.app
desktop/src-tauri/target/release/bundle/dmg/Kyoku_0.1.0_aarch64.dmg
```

仅执行默认的 `tauri build` 不会附带推理资源；完整包必须使用上述脚本。
可以对移动后的应用资源再次检查：

```bash
mortal/.venv/bin/python scripts/prepare-macos.py --verify \
  '/Applications/Kyoku.app/Contents/Resources/inference'
```

项目仍在快速迭代，公共 API 和目录结构可能调整。
