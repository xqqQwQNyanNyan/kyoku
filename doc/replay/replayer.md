# 牌谱重放状态机

`Replayer` 和 `ReplayError` 定义在 `src/replay/replayer.rs`。`Replayer` 直接消费
`convlog::Event`，不在项目内重复定义 mjai 事件。

```rust
pub const fn Replayer::new() -> Replayer

pub fn Replayer::apply(
    &mut self,
    event: &convlog::Event,
) -> Result<(), ReplayError>

pub const fn Replayer::state(&self) -> Option<&RoundState>
```

`new` 创建尚未开局的状态机。`apply` 当前只处理 `start_kyoku`、`tsumo` 和
`dahai`；其他事件返回 `ReplayError::UnsupportedEvent`。`state` 返回最新局面，
收到 `start_kyoku` 前返回 `None`。

`Replayer` 只负责匹配 `convlog::Event`、转换来源值并调用 `RoundState` 的开局、摸牌
和打牌 API。手牌、牌河、剩余摸牌数和局面阶段均由领域类型自行维护。

`ReplayError` 只保留重建状态所需的错误：暂不支持的事件、尚未开局、无法转换的
玩家或牌、无效场风与局数、无效手牌张数、手牌中不存在弃牌，以及牌山耗尽。

当前实现面向有效的四人日麻 mjai 牌谱，不校验事件阶段、行动顺序、庄家字段、摸切
一致性或完整牌张数量。

测试及覆盖说明见 [`replayer-tests.md`](replayer-tests.md)。
