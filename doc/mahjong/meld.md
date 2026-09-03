# 面子类型设计

`Meld` 定义在 `src/mahjong/meld.rs`，表示玩家当前持有的面子。它是公开枚举，
支持吃、碰、大明杠、暗杠和加杠。

吃、碰和大明杠保存组成面子的全部牌、鸣牌时取得的牌及其来源玩家。暗杠只
保存四张牌。加杠保存完成后的四张牌，并继续保留原碰牌的 `called` 和 `from`，
以便正确处理原牌河信息；它们不表示后来用于加杠的牌及其来源。

## 数据约定

`Meld` 不通过私有字段或构造器强制牌型合法。外部牌谱必须在解析和状态重放
边界完成校验，再创建 `Meld`。

所有变体的 `tiles` 都必须由调用方按 `Tile` 的领域顺序排列。同种牌中的赤牌
排在普通牌之前，因此碰和杠也必须执行排序。`Meld` 的 API 不会自动改变数组
顺序。

鸣牌来源使用 `player` 模块中经过范围校验的 `PlayerIndex`。

## API

```rust
pub fn Meld::tiles(&self) -> &[Tile]
```

返回组成面子的全部牌，不改变调用方提供的顺序。

```rust
pub const fn Meld::called(&self) -> Option<Tile>
```

返回鸣牌时从其他玩家处取得的牌，暗杠返回 `None`。对于加杠，返回原碰牌时
取得的牌。

```rust
pub const fn Meld::from(&self) -> Option<PlayerIndex>
```

返回鸣牌来源，暗杠返回 `None`。对于加杠，返回原碰牌的来源玩家。

```rust
pub const fn Meld::is_open(&self) -> bool
```

判断是否为明面子。吃、碰、大明杠和加杠返回 `true`，暗杠返回 `false`。

```rust
pub const fn Meld::is_kan(&self) -> bool
```

判断是否为杠子。大明杠、暗杠和加杠返回 `true`。

## 测试

测试及覆盖说明见 [`meld-tests.md`](meld-tests.md)。
