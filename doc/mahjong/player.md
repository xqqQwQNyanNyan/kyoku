# 玩家索引类型设计

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
