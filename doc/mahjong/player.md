# 玩家状态与索引设计

`PlayerState`、`Discard` 和 `PlayerIndex` 定义在 `src/mahjong/player.rs`。

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

## 玩家索引

`PlayerIndex` 定义在 `src/mahjong/player.rs`，表示一局四人麻将中的玩家索引。
内部 `u8` 字段私有，外部代码不能绕过校验直接构造。

有效范围为 `0..=3`。

## API

```rust
pub const fn PlayerIndex::new(value: u8) -> Option<PlayerIndex>
```

校验整数并构造玩家索引。`0..=3` 返回对应的 `PlayerIndex`，其他值返回
`None`。

```rust
pub const fn PlayerIndex::get_id(self) -> u8
```

取得玩家索引的整数值。

```rust
impl TryFrom<u8> for PlayerIndex
```

校验整数并转换为 `PlayerIndex`，失败时返回 `InvalidPlayerIndex`。这个接口适合
需要传播具体错误的解析代码。

## 错误类型

`InvalidPlayerIndex` 保留无效的原始输入，实现 `Display` 和
`std::error::Error`。

```rust
pub const fn InvalidPlayerIndex::value(self) -> u8
```

取得导致构造失败的原始输入。

## 测试

测试及覆盖说明见 [`player-tests.md`](player-tests.md)。
