# 牌谱重放状态机测试

`Replayer` 的集成测试位于 `tests/replayer.rs`。

- `start_kyoku_creates_the_round_state` 验证开局字段被写入 `RoundState`；
- `draw_and_discard_reconstruct_the_current_state` 验证 adapter 转换事件后，领域状态的
  手牌、牌河、阶段和剩余摸牌数正确；
- `a_new_start_kyoku_replaces_the_previous_round` 验证状态机不跟踪整场生命周期；
- `events_without_a_round_and_unsupported_events_are_reported` 覆盖尚未开局和暂不支持的
  事件；
- `empty_wall_error_is_translated_from_the_domain` 验证牌山耗尽由 `RoundState` 拒绝，
  `Replayer` 只翻译错误，且局面保持不变。

当前未覆盖吃、碰、杠、宝牌追加、立直、和牌和流局；这些事件尚未实现。
