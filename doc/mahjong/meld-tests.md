# 面子类型测试

`Meld` 的集成测试位于 `tests/meld.rs`。

- `tiles_returns_caller_sorted_tiles_without_reordering` 验证顺子和包含赤五的碰子
  都保留调用方给出的规范顺序，并明确 API 不替调用方重新排序。
- `called_and_from_are_absent_only_for_ankan` 覆盖五种面子的鸣牌和来源读取。
- `openness_and_kan_classification_cover_all_variants` 穷举五种枚举变体，验证
  `is_open()` 与 `is_kan()`。
- `kakan_keeps_the_original_pon_source` 验证加杠保留的是原碰牌与来源玩家。

牌型合法性、实际排序操作、座位关系和 `Event -> Meld` 转换尚未覆盖；它们应
在对应的解析与状态重放测试中验证。
