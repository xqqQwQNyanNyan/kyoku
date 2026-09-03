# 牌类型测试

`Tile` 和 `TileKind` 的测试位于 `tests/tile.rs`。测试采用 Cargo 集成测试的
形式，只能通过 crate 的公开 API 操作牌类型，因此也会检查公开接口是否足够
使用，以及私有字段是否确实阻止未校验构造。

## 测试清单

### `constructors_validate_values`

验证两个类型的构造边界：

- `Tile::new` 接受 `0` 和 `36`，拒绝 `37`；
- `TileKind::new` 接受 `0` 和 `33`，拒绝 `34`。

这项测试保护两个类型的有效值不变量。

### `integer_conversions_validate_and_preserve_values`

验证 `TryFrom<u8>` 接受合法编码、拒绝非法编码，并确认错误和 `as_u8()` 都
保留原始编码。

这项测试覆盖外部数据进入领域类型，以及领域类型显式返回底层编码的边界。

### `regular_tiles_keep_their_kind`

遍历 `0..=33` 的全部普通牌，确认转换为 `TileKind` 后编码不变。

### `red_fives_map_to_regular_five_kinds`

验证三张赤牌映射到对应普通五：

| 赤牌 | 对应牌种 |
| --- | --- |
| 赤五万 `34` | 五万 `4` |
| 赤五饼 `35` | 五饼 `13` |
| 赤五索 `36` | 五索 `22` |

### `only_red_fives_are_aka`

遍历全部有效编码，确认 `0..=33` 不是赤牌，`34..=36` 都是赤牌。

### `invalid_value_errors_expose_the_original_value`

分别制造一个无效 `Tile` 编码和无效 `TileKind` 编码，确认错误类型的
`value()` 返回原始输入，以保证上层可以给出包含上下文的错误信息。

### `tiles_sort_by_kind_with_aka_before_the_regular_five`

将包含三种赤五、对应普通五和相邻牌的数组打乱后排序，确认排序以牌种为主，
并在同种牌中把赤五排在普通五之前。例如：

```text
四万 < 赤五万 < 五万 < 六万
```

这项测试防止以后误用底层编码排序，导致赤牌被统一排到字牌之后。

## 维护要求

新增、删除或改变 `Tile`、`TileKind` 测试时，应同步更新本文件的测试清单和
覆盖说明。设计或 API 行为发生变化时，还应同步更新 `doc/tile.md`。
