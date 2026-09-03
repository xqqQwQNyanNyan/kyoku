# 玩家索引设计

`PlayerIndex` 定义在 `src/mahjong/player_index.rs`，供玩家状态、面子及后续事件
等领域类型共同使用。内部 `u8` 字段私有，有效范围为 `0..=3`。

## API

```rust
pub const fn PlayerIndex::new(value: u8) -> Option<PlayerIndex>
pub const fn PlayerIndex::get_id(self) -> u8
impl TryFrom<u8> for PlayerIndex
```

`new` 和 `TryFrom<u8>` 都会校验范围；后者失败时返回保留原始输入的
`InvalidPlayerIndex`。

```rust
pub const fn InvalidPlayerIndex::value(self) -> u8
```

## 测试

测试及覆盖说明见 [`player-index-tests.md`](player-index-tests.md)。
