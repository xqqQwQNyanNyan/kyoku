# `convlog` 的 mjai 事件格式

`convlog` 将天凤 `tenhou.net/6` 牌谱转换为按时间顺序排列的 mjai 事件流。输出文件采用 JSON Lines 格式：每一行都是一个独立的 JSON 事件对象，而不是包含全部事件的 JSON 数组。

所有事件都包含字符串字段 `type`，用于区分事件类型。玩家编号 `actor`、`target` 和 `oya` 的取值范围都是 `0..3`；四元素数组均按照玩家 `0、1、2、3` 的顺序排列。

## 事件及字段

| 事件 `type` | 字段 | 类型 | 含义 |
|---|---|---|---|
| `none` | `type` | `"none"` | 无动作，主要供 AI 决策接口使用；牌谱转换通常不会生成 |
| `start_game` | `type` | `"start_game"` | 整场牌局开始 |
|  | `names` | `[String; 4]` | 四名玩家的名称 |
|  | `kyoku_first` | `u8` | akochan 兼容字段；本库中东风战为 `4`，半庄战为 `0` |
|  | `aka_flag` | `bool` | 是否启用赤宝牌 |
| `start_kyoku` | `type` | `"start_kyoku"` | 一局开始 |
|  | `bakaze` | `Tile` | 场风：`E`、`S`、`W` 或 `N` |
|  | `dora_marker` | `Tile` | 开局时翻开的宝牌指示牌 |
|  | `kyoku` | `u8` | 当前场风内的局数，从 `1` 开始；例如东一局为 `1` |
|  | `honba` | `u8` | 本场数 |
|  | `kyotaku` | `u8` | 场上的立直棒数量 |
|  | `oya` | `u8` | 庄家编号 |
|  | `scores` | `[i32; 4]` | 开局时四名玩家的点数 |
|  | `tehais` | `[[Tile; 13]; 4]` | 四名玩家的初始手牌，每家 13 张 |
| `tsumo` | `type` | `"tsumo"` | 摸牌事件 |
|  | `actor` | `u8` | 摸牌的玩家 |
|  | `pai` | `Tile` | 摸到的牌 |
| `dahai` | `type` | `"dahai"` | 打牌事件 |
|  | `actor` | `u8` | 出牌的玩家 |
|  | `pai` | `Tile` | 打出的牌 |
|  | `tsumogiri` | `bool` | `true` 表示摸切；`false` 表示从原手牌中切出 |
| `chi` | `type` | `"chi"` | 吃牌事件 |
|  | `actor` | `u8` | 吃牌的玩家 |
|  | `target` | `u8` | 打出被吃牌的玩家 |
|  | `pai` | `Tile` | 从 `target` 获得的牌 |
|  | `consumed` | `[Tile; 2]` | `actor` 从自己手牌中使用的两张牌 |
| `pon` | `type` | `"pon"` | 碰牌事件 |
|  | `actor` | `u8` | 碰牌的玩家 |
|  | `target` | `u8` | 打出被碰牌的玩家 |
|  | `pai` | `Tile` | 从 `target` 获得的牌 |
|  | `consumed` | `[Tile; 2]` | `actor` 从自己手牌中使用的两张牌 |
| `daiminkan` | `type` | `"daiminkan"` | 大明杠事件 |
|  | `actor` | `u8` | 大明杠的玩家 |
|  | `target` | `u8` | 打出被杠牌的玩家 |
|  | `pai` | `Tile` | 从 `target` 获得的牌 |
|  | `consumed` | `[Tile; 3]` | `actor` 自己持有的另外三张牌 |
| `kakan` | `type` | `"kakan"` | 加杠事件 |
|  | `actor` | `u8` | 加杠的玩家 |
|  | `pai` | `Tile` | 新加入原有碰子的第四张牌 |
|  | `consumed` | `[Tile; 3]` | 原来构成碰子的三张牌 |
| `ankan` | `type` | `"ankan"` | 暗杠事件 |
|  | `actor` | `u8` | 暗杠的玩家 |
|  | `consumed` | `[Tile; 4]` | 构成暗杠的四张牌 |
| `dora` | `type` | `"dora"` | 新宝牌指示牌事件 |
|  | `dora_marker` | `Tile` | 杠后新翻开的宝牌指示牌 |
| `reach` | `type` | `"reach"` | 宣告立直；后面通常紧跟一次 `dahai` |
|  | `actor` | `u8` | 宣告立直的玩家 |
| `reach_accepted` | `type` | `"reach_accepted"` | 立直正式成立，表示立直棒已支付 |
|  | `actor` | `u8` | 立直成立的玩家 |
| `hora` | `type` | `"hora"` | 和牌事件；双响时可以连续出现多个 `hora` |
|  | `actor` | `u8` | 和牌者 |
|  | `target` | `u8` | 荣和时为放铳者；自摸时通常与 `actor` 相同 |
|  | `deltas` | `Option<[i32; 4]>` | 本次和牌造成的四家点数变化；可省略 |
|  | `ura_markers` | `Option<Vec<Tile>>` | 里宝牌指示牌列表；可省略 |
| `ryukyoku` | `type` | `"ryukyoku"` | 流局事件 |
|  | `deltas` | `Option<[i32; 4]>` | 流局造成的四家点数变化；可省略 |
| `end_kyoku` | `type` | `"end_kyoku"` | 当前一局结束，无其他字段 |
| `end_game` | `type` | `"end_game"` | 整场牌局结束，无其他字段 |

`Option` 字段为 `None` 时不会出现在输出 JSON 中。天凤转换器生成 `hora` 和 `ryukyoku` 时会填入 `deltas`；生成 `hora` 时也会填入 `ura_markers`。

## 牌的表示

JSON 中的 `Tile` 使用字符串表示。

| 牌类 | 字符串 |
|---|---|
| 万子 | `1m`～`9m` |
| 筒子 | `1p`～`9p` |
| 索子 | `1s`～`9s` |
| 风牌 | `E`（东）、`S`（南）、`W`（西）、`N`（北） |
| 三元牌 | `P`（白）、`F`（发）、`C`（中） |
| 赤五万 | `5mr` |
| 赤五筒 | `5pr` |
| 赤五索 | `5sr` |
| 未知牌 | `?`，主要作为转换过程中的临时占位符 |

## 示例

```json
{"type":"pon","actor":2,"target":0,"pai":"5p","consumed":["5p","5pr"]}
```

该事件表示玩家 2 碰了玩家 0 打出的普通 `5p`，并从自己手牌中使用普通 `5p` 和赤 `5pr`。
