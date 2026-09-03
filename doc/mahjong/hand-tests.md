# 手牌状态测试

`Hand` 的集成测试位于 `tests/hand.rs`。

- `constructor_accepts_thirteen_and_fourteen_effective_tiles` 验证两种允许的张数；
- `constructor_sorts_concealed_tiles_by_domain_order` 验证暗牌排序，包括赤五顺序；
- `melds_keep_their_input_order` 验证面子顺序不会被改变；
- `kans_count_as_three_effective_tiles` 覆盖杠的等效张数计算；
- `constructor_rejects_every_other_effective_size_with_context` 覆盖过少、过多和带面子
  的错误输入，并验证错误上下文；
- `constructor_does_not_apply_additional_legality_checks` 明确同种牌超过四张不会在本层
  被拒绝。

当前未覆盖摸牌、打牌和副露导致的状态迁移；本次 API 只提供不可变的手牌快照。
