# 手牌状态设计

`Hand` 定义在 `src/mahjong/hand.rs`，保存一名玩家当前的暗牌和面子。字段私有，
外部只能通过构造函数创建手牌。

## 不变量

- `concealed` 始终按照 `Tile` 的领域顺序排列；
- 等效张数只能为 13 或 14；
- 每个面子按三张计算，杠的第四张不额外占用手牌结构位。

构造器不检查同种牌是否超过四张、`Meld` 的内容是否合法，或手牌是否能由真实
对局产生。

## API

```rust
pub fn Hand::new(
    concealed: Vec<Tile>,
    melds: Vec<Meld>,
) -> Result<Hand, InvalidHandSize>
```

校验等效张数并整理暗牌。

```rust
pub fn Hand::concealed(&self) -> &[Tile]
pub fn Hand::melds(&self) -> &[Meld]
pub fn Hand::effective_tile_count(&self) -> usize
```

分别读取暗牌、面子和等效张数。返回切片，调用方不能绕过 `Hand` 修改内部状态。

```rust
pub const fn InvalidHandSize::concealed_count(self) -> usize
pub const fn InvalidHandSize::meld_count(self) -> usize
pub const fn InvalidHandSize::effective_tile_count(self) -> usize
```

构造失败时，可从错误中取得参与计算的数量。

## 测试

测试及覆盖说明见 [`hand-tests.md`](hand-tests.md)。
