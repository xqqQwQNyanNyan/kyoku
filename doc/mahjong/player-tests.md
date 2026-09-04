# 玩家状态测试

玩家相关类型的集成测试位于 `tests/player.rs`。

- `discard_exposes_its_tile_and_flags` 验证弃牌保留牌和三个布尔标记，同时覆盖标记
  全为真和全为假的情况；
- `player_state_exposes_hand_score_and_discard_order` 验证玩家状态保留手牌、负分和
  牌河顺序；
- `draw_and_discard_keep_the_hand_and_river_in_sync` 验证玩家摸牌修改手牌，打牌同时
  修改手牌并追加带摸切标记的牌河记录。

当前尚未覆盖鸣牌、立直和点数变化。
