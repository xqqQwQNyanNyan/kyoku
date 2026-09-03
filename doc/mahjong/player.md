# 玩家状态设计

`PlayerState` 和 `Discard` 定义在 `src/mahjong/player.rs`。

## 玩家状态

`PlayerState` 是一名玩家在当前局中的快照，保存手牌、点数和牌河。牌河按照实际
打出时间排列。当前类型不校验手牌、点数和牌河能否由同一段合法对局产生。

```rust
pub fn PlayerState::new(
    hand: Hand,
    score: i32,
    discards: Vec<Discard>,
) -> PlayerState

pub const fn PlayerState::hand(&self) -> &Hand
pub const fn PlayerState::score(&self) -> i32
pub fn PlayerState::discards(&self) -> &[Discard]
```

点数使用 `i32`，允许表达负分。手牌与牌河只通过不可变引用暴露。

## 弃牌记录

`Discard` 保存打出的牌，以及它是否为摸切、立直宣言牌和是否已被鸣走。

```rust
pub const fn Discard::new(
    tile: Tile,
    tsumogiri: bool,
    riichi: bool,
    called: bool,
) -> Discard

pub const fn Discard::tile(self) -> Tile
pub const fn Discard::is_tsumogiri(self) -> bool
pub const fn Discard::is_riichi(self) -> bool
pub const fn Discard::is_called(self) -> bool
```

这些标记彼此独立；构造器不推断或校验事件顺序。

## 测试

测试及覆盖说明见 [`player-tests.md`](player-tests.md)。
