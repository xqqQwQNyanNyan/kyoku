# 牌类型设计

`Tile` 和 `TileKind` 是麻将领域层的基础值类型，定义在
`src/mahjong/tile.rs`。两者都使用私有的 `u8` 保存编码，外部代码不能绕过
校验直接构造。

## `Tile`

`Tile` 表示一张牌，并保留赤牌信息。有效编码为 `0..=36`：

| 编码 | 含义 |
| --- | --- |
| `0..=8` | 一万至九万 |
| `9..=17` | 一饼至九饼 |
| `18..=26` | 一索至九索 |
| `27..=33` | 东、南、西、北、白、发、中 |
| `34` | 赤五万 |
| `35` | 赤五饼 |
| `36` | 赤五索 |

`Tile` 的相等性包含赤牌信息，因此普通五万和赤五万不是同一张 `Tile`。

### 排序

`Tile` 手动实现 `PartialOrd` 和 `Ord`，不能直接按内部编码排序。排序时先比较
对应的 `TileKind`，同种牌再将赤牌排在普通牌之前。例如：

```text
四万 < 赤五万 < 五万 < 六万
```

普通五和赤五不能比较为相等，因为这会破坏 `Ord` 与 `Eq` 的一致性。

### API

```rust
pub const fn Tile::new(value: u8) -> Option<Tile>
```

校验编码并构造 `Tile`。有效范围为 `0..=36`，无效时返回 `None`。

```rust
pub const fn Tile::as_u8(self) -> u8
```

显式取得内部编码。

```rust
pub const fn Tile::kind(self) -> TileKind
```

取得忽略赤牌差异后的牌种类。

```rust
pub const fn Tile::is_aka(self) -> bool
```

判断是否为赤五万、赤五饼或赤五索。

```rust
impl TryFrom<u8> for Tile
```

校验整数并转换为 `Tile`，失败时返回 `InvalidTile`。与 `new` 相比，这个接口
适合需要传播具体错误的解析代码。

`Tile::MAX_VALUE` 为当前允许的最大编码 `36`。

## `TileKind`

`TileKind` 表示忽略赤牌差异后的牌种类，有效编码为 `0..=33`，编码顺序与
`Tile` 的前三十四种普通牌相同。赤牌映射如下：

| `Tile` 编码 | `TileKind` 编码 |
| --- | --- |
| 赤五万 `34` | 五万 `4` |
| 赤五饼 `35` | 五饼 `13` |
| 赤五索 `36` | 五索 `22` |

`TileKind` 可以按内部编码直接排序，因此派生实现了 `PartialOrd` 和 `Ord`。

### API

```rust
pub const fn TileKind::new(value: u8) -> Option<TileKind>
```

校验编码并构造 `TileKind`。有效范围为 `0..=33`，无效时返回 `None`。

```rust
pub const fn TileKind::as_u8(self) -> u8
```

显式取得内部编码。

```rust
impl From<Tile> for TileKind
```

等价于调用 `Tile::kind()`。这个转换不会失败，因为每个合法的 `Tile` 都有
对应的 `TileKind`。

```rust
impl TryFrom<u8> for TileKind
```

校验整数并转换为 `TileKind`，失败时返回 `InvalidTileKind`。

`TileKind::MAX_VALUE` 为当前允许的最大编码 `33`。

## 转换边界

项目不实现 `From<Tile> for u8` 或 `From<TileKind> for u8`。领域类型转换为原始
编码时必须显式调用 `as_u8()`，避免 `.into()` 隐藏领域边界。

原始整数转换为领域类型时必须使用 `new` 或 `TryFrom<u8>`，不能构造未校验的
`Tile` 或 `TileKind`。

## 错误类型

`InvalidTile` 和 `InvalidTileKind` 保存导致构造失败的原始编码，均实现
`Display` 和 `std::error::Error`。可以通过各自的 `value()` 方法取得该编码。

## 测试

测试位于 `tests/tile.rs`，各项测试的目的和覆盖范围记录在
[`tile-tests.md`](tile-tests.md)。
