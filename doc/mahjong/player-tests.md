# 玩家状态与索引测试

玩家相关类型的集成测试位于 `tests/player.rs`。

- `discard_exposes_its_tile_and_flags` 验证弃牌保留牌和三个布尔标记，同时覆盖标记
  全为真和全为假的情况；
- `player_state_exposes_hand_score_and_discard_order` 验证玩家状态保留手牌、负分和
  牌河顺序；

- `constructor_accepts_only_four_player_indices` 验证 `0..=3` 全部合法，并覆盖
  紧邻上界的 `4` 和 `u8::MAX` 两个非法边界。
- `integer_conversion_reports_the_invalid_value` 验证 `TryFrom<u8>` 的成功路径，
  以及错误保留原始值并给出明确说明。
- `get_id_returns_the_original_value` 遍历四个合法索引，验证整数往返不变。

当前类型只表达四人麻将的绝对玩家索引，不表达玩家之间的相对方位。三人麻将
等规则变体尚未建模。玩家状态也尚未覆盖摸牌、打牌、鸣牌和点数变化等状态迁移。
