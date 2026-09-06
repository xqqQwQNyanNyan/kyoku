# Kyoku

一个用 Rust 编写的日麻牌谱复盘助手。

这个项目想解决的不是“让 LLM 自己判断麻将怎么打”，而是把几种不同的能力组合起来：

* Rust 负责牌局状态、规则和确定性计算；
* Mortal 提供打法上的模型判断；
* LLM 负责调用工具、整理证据并解释结果；
* GUI 负责牌谱浏览、局面查看和交互式复盘。

最终希望用户可以导入一份牌谱，然后直接问：

* 为什么这里不应该切六万？
* 这两个切牌在牌效率上差多少？
* Mortal 为什么更喜欢另一个选择？
* 这里应该继续进攻还是开始防守？
* 整场牌谱里哪些决策最值得复盘？

系统尽量不只给结论，而是说明这个结论来自规则计算、Mortal、统计资料，还是进一步的推断。

## How it works

整个项目大致会沿着下面这条链路工作：

```text
game log
    ↓
parser / convlog
    ↓
game state
    ↓
mahjong analysis
    ↓
Mortal / other tools
    ↓
Agent
    ↓
GUI
```

目前已实现局面重建、确定性麻将分析、本地 Mortal 推理、单局面复盘、整场决策浏览及命令行 Agent 问答；GUI 尚未接入。

### Mahjong core

Rust 负责维护可信的麻将状态。

领域层会表示诸如：

* 牌；
* 手牌和副露；
* 玩家状态；
* 牌河；
* 当前局和局面阶段；
* 点数和其他牌桌信息。

状态对象自己维护合法性和状态转换，而不是把所有规则都堆到牌谱解析代码里。

对于同样的输入和规则，这一层应该得到稳定、可复现的结果。

### Game log replay

牌谱目前使用 `convlog` 提供的 mjai 事件作为输入。

`Replayer` 负责把外部事件翻译成领域对象上的操作：

```text
convlog::Event
      ↓
   Replayer
      ↓
RoundState / PlayerState / Hand
```

`Replayer` 本身尽量只承担适配工作。

真正的麻将状态和约束留在领域模型里，这样以后无论输入来自天凤、雀魂还是别的格式，上层分析代码都不需要跟着牌谱格式变化。

可以用开发用的 `replay` 二进制检查本地或远程 Tenhou 牌谱：

```bash
cargo run --bin replay -- fixtures/tenhou/ranked_game.json
cargo run --bin replay -- --full-state fixtures/tenhou/rinshan.json
cargo run --bin replay -- --kyoku E2 --only hora,kan,dora,ryukyoku fixtures/tenhou/ranked_game.json
cargo run --bin replay -- --from 120 --to 140 fixtures/tenhou/ranked_game.json
cargo run --bin replay -- --state-at 120 fixtures/tenhou/ranked_game.json
cargo run --bin replay -- 'https://tenhou.net/0/?log=<log-id>&tw=0'
cargo run --bin replay -- '<log-id>'
```

输入使用 `-` 时从标准输入读取。`--event`、`--from`、`--to` 和 `--state-at` 使用输出中的零基全局事件编号，范围端点包含在结果内；`--state-at` 捕获指定事件应用后的完整局面。`--kyoku E2` 会包含东二局的所有本场，写成 `E2.1` 时只选择一本场。`--only` 只过滤显示，事件仍会完整进入回放器；局开始、局结束和比赛结束摘要会保留。命令默认打印逐事件状态变化；回放失败时仍会显示事件索引、附近事件和最后一个有效局面。

回放用的 Tenhou JSON 牌谱来自 `convlog` 仓库的 `convlog/tests/testdata`，本地副本放在
`fixtures/tenhou/`。这些 fixture 被 Git 忽略，不属于项目源码；在新的工作区运行回放
测试前，需要先从对应的 `convlog` checkout 复制这些 JSON 文件到该目录。当前目录中的
牌谱覆盖双响、流局、抢杠、岭上摸牌、连续杠和复杂鸣牌等状态转移。

### Mahjong analysis

在可靠的局面状态之上，会逐步实现确定性的麻将分析工具，例如：

* 向听数；
* 有效牌；
* 牌效率；
* 打点和符数；
* 剩余枚数；
* 危险度和防守相关信息。

这些问题如果能够由程序确定计算，就不交给 LLM 猜。

### Mortal

Mortal 已作为独立的本地 Python 进程接入，当前支持四人 Mortal V4 的 CPU 推理。
Rust 按顺序发送 mjai 事件，并获取推荐动作、候选动作 Q 值、引擎向听数及振听状态。
对手起手牌和摸牌会遮蔽，后续仍沿真实牌谱推进，不自动执行模型建议。

第一次使用需要 Python 3.11+（推荐 3.12）、Rust、Git 和 curl：

```bash
bash scripts/setup-mortal.sh /path/to/python3.12
cargo run --bin mortal -- --player 0 --event 2 fixtures/tenhou/ranked_game.json
cargo run --bin mortal -- --player 0 fixtures/tenhou/ranked_game.json
```

准备脚本将 Python 虚拟环境、固定版本的官方 Mortal 源码及模型放在根目录的
`mortal/` 下，并编译 `libriichi`。这些本地依赖被 Git 忽略，目录说明和许可记录可提交；
具体结构见 [`mortal/README.md`](mortal/README.md)。支持 macOS 和 Linux，需要联网下载依赖和约 125 MiB
的模型。默认使用 [Yuchen1457/mortal-582500 社区四麻权重](https://huggingface.co/Yuchen1457/mortal-582500)，
下载后校验 SHA-256；这不是 Mortal 官网的官方权重，不据此声称相同棋力。
Mortal 源码与模型的许可及来源分别见[官方仓库](https://github.com/Equim-chan/Mortal)
和模型发布页。

`--player` 是整场不变的玩家索引 `0..3`；`--event` 与 `replay` 共用零基全局事件编号，
表示该事件应用后的决策，之前的事件仍完整送入引擎。省略它则输出整场该玩家的判断。
输入支持本地 Tenhou JSON 或标准输入 `-`；`--python`、`--runtime`、`--model` 可以
覆盖默认路径。命令输出实际模型文件的 SHA-256，便于确认复盘使用了哪个模型。

程序接口为 `kyoku::mortal::{Mortal, MortalConfig}`：`start` 加载一次模型，`react`
逐事件返回 `Option<Decision>`，`finish` 关闭进程并检查退出状态。没有决策机会时返回
`None`；主动跳过鸣牌则返回推荐动作为 `convlog::Event::None` 的 `Some(Decision)`。
启动或响应超过 60 秒、进程提前退出、JSON 或动作掩码不合法时会返回错误，失败会话
不可继续使用。调用方仍需用 Replay 校验输入事件；推理适配不代替领域状态机。

`Decision::recommended` 是引擎的最终推荐，**不保证等于 Q 值最大的候选动作**。
桥接启用了和牌规则保护（`enable_rule_based_agari_guard=True`），可能改变最终动作，
而 Q 表仍保留原始评价。两者冲突时以 `recommended` 为准；Agent 和其他调用方不得
用 argmax Q 或排序后的首项代替最终推荐。CLI 的 `Mortal:` 行同样优先于候选表。
这符合[官方 FAQ](https://github.com/Equim-chan/mjai-reviewer/blob/master/faq.md#mortal-the-single-line-output-and-the-table-are-in-conflict-is-it-a-bug) 的约定。

Q 值是原始模型输出，不是概率或期望点数。杠牌种选择保留独立评价，不与主动作的
Q 值混排。CLI 对单个杠牌种候选只显示主层 `Kan` 的 Q；多个候选才展开第二层评价，
用于比较“杠哪个”。API 始终保留两层原始值。当前不计算整场评分、顺位预测或自动识别
失误，也不依赖 GRP 权重。

接口、事件协议和错误边界见 [`docs/mortal/mortal.md`](docs/mortal/mortal.md)，
自动测试及真实模型验证见 [`docs/mortal/mortal-tests.md`](docs/mortal/mortal-tests.md)。

它适合回答：

* 哪个动作更值得选择；
* 不同候选动作之间的倾向有多大；
* 某个局面的整体价值如何。

但 Mortal 不是规则引擎。

牌是否合法、当前状态是什么、某个确定性指标是多少，仍然由 Rust 这一层负责。

### Review

`review` 将同一事件后的玩家可见局面、切牌效率和 Mortal 判断汇总展示：

```bash
cargo run --bin review -- --player 0 --event 2 fixtures/tenhou/ranked_game.json
cargo run --bin review -- --player 1 --event 30 fixtures/tenhou/complex_nakis.json

# 省略 --event，列出整场该玩家的行动机会。
cargo run --bin review -- --player 0 fixtures/tenhou/ranked_game.json
```

运行前按上面的 Mortal 说明准备本地环境。输入支持本地 Tenhou JSON 和标准输入 `-`，
`--player` 必填；指定 `--event` 查看单局面，省略则列出整场决策。
事件编号与 `replay`、`mortal` 一致，表示事件应用后的局面。
支持相同的 `--python`、`--runtime`、`--model` 路径覆盖参数。

程序接口为 `kyoku::review::review_at`，返回结构化 `Review`；命令行只负责格式化。
结果包含自家暗牌、四家公开信息、Mortal 当前切牌候选的向听与进张、模型身份和原始判断，
不暴露对手暗牌。不可见枚数不是实际牌山剩余枚数，完成牌形也不代表可以合法和牌。
无行动机会仍返回局面；吃碰、和牌或跳过等决策保留 Mortal 输出，只有切牌候选附带牌效率。
单局面查询 `review_at` 每次独立加载模型。整场接口 `review_game` 只启动一次 Mortal，
按真实牌谱顺序推理，返回 `GameReview`；`decisions()` 列出行动机会，`at_event(N)`
读取缓存的 `DecisionPoint`，切换时无需重新回放或计算。缓存仅存在当前进程内存中。

列表包含局数、本场、自家手番、实际动作与 Mortal 最终推荐；手番按自家牌河长度加一
计算，鸣牌响应标在下一次出牌手番，杠不单独增加手番。仅收录 Mortal 返回的行动机会，
包含单候选和跳过，不做失误评分或排序。实际动作单独存放在 `DecisionPoint.actual`，
不进入当时的 `Review` 证据；他家抢先鸣牌或和牌、流局原因不明及牌谱截断时，
无法确认的选择明确标为“无法确定”，不当作跳过。

接口和行为约定见 [`docs/review/review.md`](docs/review/review.md)，
验证说明见 [`docs/review/review-tests.md`](docs/review/review-tests.md)。

### Agent

`agent` 已接入 OpenAI Responses API，可以围绕指定局面提问和追问。
先按 Mortal 的说明准备本地环境。`agent` 自动读取当前工作目录的 `.env`；
在项目根目录把 `.env.example` 复制为 `.env`，填写服务地址、模型名和密钥即可。
中转站使用 `KYOKU_OPENAI_ENDPOINT`、`OPENAI_MODEL`、`AGENT_API_KEY`；
官方服务省略地址覆盖，使用 `OPENAI_MODEL` 和 `OPENAI_API_KEY`。
模型须支持 Responses 工具调用。配置优先级为命令行参数、已有环境变量、`.env`；
`.env` 已被 Git 忽略，示例文件不含真实密钥。

```bash
cargo run --bin agent -- --player 0 --event 2 \
  --question '比较这里的候选切牌，说明向听、进张和 Mortal 的倾向。' \
  fixtures/tenhou/ranked_game.json

# 不带 --question 就进入交互模式；同一局面只运行一次 Mortal。
cargo run --bin agent -- --player 0 --event 2 fixtures/tenhou/ranked_game.json

# 整场浏览：模型加载一次，切换局面使用缓存。
cargo run --bin agent -- --player 0 --browse fixtures/tenhou/ranked_game.json
```

交互中可以继续问“这就是牌山剩余枚数吗？”，输入 `/evidence` 查看原始 JSON 证据，
输入 `/quit` 退出。单局面交互中的 `/evidence` 同样无需 LLM 配置，问答时才创建会话。
`--question` 搭配 `--interactive` 可以先回答一问再继续交互。
牌谱从标准输入 `-` 读取时，只支持单次 `--question`。

浏览模式先列出整场决策并选择第一项。用 `/select N` 按列表中的全局事件编号选择，
`/next`、`/prev` 切换，`/show` 查看局面、牌效率和 Mortal 候选，`/list` 重看列表，
直接输入问题即可问答。切换到其他局面会清空问答历史；切回也重新取证，不复用旧对话。
越界或无效选择保留当前局面和问答。浏览、`/show` 和 `/evidence` 不需要 LLM 配置；
仅问答需要按上面的说明配置服务。`--browse` 只接受文件，不能与 `--event` 或 `--question` 同用。

默认官方地址使用 `OPENAI_API_KEY`。通过 `--endpoint` 或 `KYOKU_OPENAI_ENDPOINT`
覆盖地址时，只读取独立的 `AGENT_API_KEY`，不会回退到 `OPENAI_API_KEY`；
本地无认证服务可不设置 `AGENT_API_KEY`，自定义远程服务则需设置。

首次回答前必须通过 `get_review` 工具取得当前局面的证据；追问复用这份证据及对话历史。
LLM 服务收到问题、自家暗牌、公开信息、牌效率和 Mortal 判断，不会收到完整牌谱、
对手暗牌或未来事件。会话仅保存在本地进程内存中，请求使用 `store: false`；这不等于
服务商承诺零数据留存。不要将密钥写进命令行参数或提交到仓库。

回答要求区分【计算】【Mortal】【推测】，不能把 Q 值当成概率或编造推荐原因；
这些是提示词约束，不保证每句解释都正确，具体数字可以用 `/evidence` 核对。
每次问答仅使用当前选定局面的证据，不向 LLM 提供整场缓存、实际后续动作或其他局面的对话。
当前不提供整场自动找错或押退风险计算。

配置、工具协议和错误约定见 [`docs/agent/agent.md`](docs/agent/agent.md)，
测试及人工验收见 [`docs/agent/agent-tests.md`](docs/agent/agent-tests.md)。

Agent 负责把这些工具组合起来，后续会逐步扩展工具编排能力。

例如用户问：

> 为什么这里不应该切 6m？

Agent 可以先读取当前局面，再调用牌效率、剩余枚数、Mortal 等工具，最后根据得到的结果组织解释。

LLM 的主要工作是：

* 理解用户的问题；
* 决定需要哪些工具；
* 综合不同来源的结果；
* 把分析解释成人能读懂的语言；
* 支持继续追问。

以后也可以加入麻将书、历史牌谱和统计数据作为额外依据，但这些不属于最基础的依赖。

### GUI

最终会提供一个桌面界面，用来：

* 导入牌谱；
* 浏览牌局；
* 查看某一巡的完整局面；
* 对比候选动作；
* 展示计算和模型结果；
* 和 Agent 继续讨论这个局面。

具体 GUI 技术方案会在核心能力稳定之后决定。

## Project structure

目前项目还在早期阶段，目录结构会随着需求继续调整。

代码会尽量保持几个明确的边界：

```text
game log / external formats
            ↓
         replay
            ↓
      mahjong domain
            ↓
        analysis
            ↓
   Mortal / Agent / GUI
```

领域层不应该依赖 GUI、具体 LLM SDK 或牌谱来源格式。

上层可以依赖领域层，领域层尽量不知道上层的存在。

## Current status

目前包含麻将领域模型、真实牌谱 Replay、确定性麻将分析、本地 Mortal 推理、单局面复盘、整场决策浏览及 Agent 问答。
`mortal` 可以获取指定玩家逐事件的模型建议，运行时不依赖 Kyoku 分析层；
`review` 在指定事件上汇总可见局面、Kyoku 切牌分析和 Mortal 判断。
分析层已支持向听、基础牌效率、役种判断和计分；`agent` 通过 Responses API
调用当前选定局面的工具并生成中文解释，支持追问和在整场缓存间切换。更丰富的工具编排及 GUI 尚未实现。

## Roadmap

大致的开发顺序是：

```text
Mahjong core
    ↓
Replay
    ↓
Mahjong analysis
    ↓
Mortal
    ↓
Agent
    ↓
GUI
```

第一版不追求一次把所有设想做完。

只要能够跑通：

```text
牌谱
 → 局面重建
 → 麻将分析
 → Mortal
 → Agent
 → GUI
```

这一整条链路，就已经是一个完整的可用版本。

之后再考虑更大的功能，例如：

* 自动找出整场牌谱中值得复盘的决策；
* 麻将书和文章检索；
* 历史牌谱统计；
* 相似局面搜索；
* 更完整的评估体系。

## Development

需要安装稳定版 Rust 工具链。

常用检查：

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

项目仍在开发中，API 和目录结构暂时都可能继续调整。

## Why this project

麻将复盘里其实混着几种完全不同的问题。

有些问题有明确答案，例如：

* 现在是什么向听；
* 有哪些有效牌；
* 某张牌还剩多少；
* 当前动作是否合法。

这些问题应该交给程序计算。

另外一些问题没有唯一正确答案：

* 速度和打点怎么权衡；
* 要不要继续押；
* 两个都合理的切牌哪个更好；
* 当前点况下应该采用什么策略。

这些问题更适合参考 Mortal 一类模型。

最后还有一个很重要的问题：

> 为什么？

一个模型可能告诉你它更喜欢某个动作，但这并不等于完成了一次好的复盘。

这个项目希望把确定性计算、模型判断和自然语言解释分开，然后再把它们组合起来。

程序负责算清楚，模型负责提供判断，Agent 负责把这些东西讲明白。
