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

目前已实现局面重建、确定性麻将分析和本地 Mortal 推理；Agent 与 GUI 尚未接入。

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

Q 值是原始模型输出，不是概率或期望点数。杠牌种选择保留独立评价，不与主动作的
Q 值混排。当前不计算整场评分、顺位预测或自动识别失误，也不依赖 GRP 权重。

接口、事件协议和错误边界见 [`docs/mortal/mortal.md`](docs/mortal/mortal.md)，
自动测试及真实模型验证见 [`docs/mortal/mortal-tests.md`](docs/mortal/mortal-tests.md)。

它适合回答：

* 哪个动作更值得选择；
* 不同候选动作之间的倾向有多大；
* 某个局面的整体价值如何。

但 Mortal 不是规则引擎。

牌是否合法、当前状态是什么、某个确定性指标是多少，仍然由 Rust 这一层负责。

### Agent

Agent 负责把这些工具组合起来。

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

目前包含麻将领域模型、真实牌谱 Replay、确定性麻将分析和本地 Mortal 推理命令。
当前可以从 Tenhou 牌谱获取指定玩家逐事件的模型建议，运行时不依赖 Kyoku 分析层。
分析层已支持向听、基础牌效率、役种判断和计分；Agent 的工具编排、
自然语言解释和 GUI 尚未实现。

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
