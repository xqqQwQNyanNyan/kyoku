# 本地 Mortal 环境

在项目根目录运行 `bash scripts/setup-mortal.sh /path/to/python3.12`，然后使用
`cargo run --bin mortal -- --player 0 --event 2 fixtures/tenhou/ranked_game.json`。
脚本支持 macOS 和 Linux，Python 3.11+，推荐 3.12。

```text
mortal/
├── runtime/                 官方 Mortal checkout，包含本地编译的 libriichi
├── .venv/                   Python 推理环境
└── models/
    ├── mortal_582500.pth     社区四麻权重
    ├── MODEL_CARD.md        发布者的原始模型说明
    ├── Mortal-LICENSE       发布者随模型附带的许可全文
    └── model-manifest.json  发布者的模型身份及校验信息
```

`runtime/`、`.venv/`、权重和未完成下载被 Git 忽略；说明、许可证和模型清单保留在仓库。
Python 虚拟环境包含绝对路径，换工作目录或电脑时应重新创建。
Kyoku 的 Rust 适配代码和 Python 桥接脚本位于 `src/mortal/`。
接入设计见 [`docs/mortal/mortal.md`](../docs/mortal/mortal.md)，
测试说明见 [`docs/mortal/mortal-tests.md`](../docs/mortal/mortal-tests.md)。

## 来源与许可

官方代码来自 [Equim-chan/Mortal](https://github.com/Equim-chan/Mortal)，固定提交
`0cff2b52982be5b1163aa9a62fb01f03ce91e0d2`。作者声明代码为 **AGPL-3.0-or-later**，
Copyright (C) 2021-2022 Equim；参见该提交的
[README](https://github.com/Equim-chan/Mortal/blob/0cff2b52982be5b1163aa9a62fb01f03ce91e0d2/README.md)
和 [LICENSE](https://github.com/Equim-chan/Mortal/blob/0cff2b52982be5b1163aa9a62fb01f03ce91e0d2/LICENSE)。
官方 README 将 logo 与其他素材另列为 CC BY-SA 4.0，不能把整个上游仓库的所有文件
一概标记成 AGPL；重新打包时还需保留各依赖自身的许可。

权重来自 [Yuchen1457/mortal-582500](https://huggingface.co/Yuchen1457/mortal-582500)，
固定发布修订为 `7386c9f5c751a3ea75efea99737cef5a5ef950f1`。它是社区 Mortal V4 四麻
checkpoint，不是 Mortal 官网使用的官方权重。发布页元数据标记 `agpl-3.0`，
正文明确写为 **AGPL-3.0-or-later**，并声明该次发布已获再分发授权。
上述三个说明文件原样取自该修订，核对日期为 2026-09-06。
这里记录发布方的许可声明，并未独立核验其完整授权链。

模型 SHA-256 为：

```text
738e0d6e3c0ce9671629554ad39abd147d2ffbac676e80b194c83f2acc0fea20
```

`model-manifest.json` 中的 `runtime_manifest_sha256` 属于发布者的运行环境记录；
本项目实际运行的官方源码版本以上面的 Git 提交为准，不能将两者视为同一个校验值。

## 公开分发

AGPL 允许按条件复制、修改及再分发，也允许商业使用；公开代码本身是允许的。
再分发时须保留作者、版权、许可和免责声明；修改受许可覆盖的作品时须标明修改，
并遵守 AGPL 的许可要求。分发受覆盖的二进制还需按第 6 条提供相应源码。
修改后的受覆盖程序通过网络与用户交互时，第 13 条要求向这些用户提供相应源码。
具体以 [AGPL 正文](https://www.gnu.org/licenses/agpl.en.html) 为准。

按发布者的声明，这份原样权重可以在遵守所声明许可的条件下再分发，应一并保留
模型说明、许可和来源。本项目目前通过脚本从发布方下载权重，不将大文件或本机
虚拟环境提交到 Git，也不将本机编译产物打包为公开发行版。

独立进程是工程边界，不能仅据此认定 Kyoku 不受 AGPL 组合／衍生作品要求影响；
还要看实际结合方式，参见 [GNU FAQ](https://www.gnu.org/licenses/gpl-faq.html.en#MereAggregation)。
当前 Kyoku 尚未设置项目级 `LICENSE`；这些第三方许可附件只说明对应组件，
不等于已经为 Kyoku 自身选择了许可证。
