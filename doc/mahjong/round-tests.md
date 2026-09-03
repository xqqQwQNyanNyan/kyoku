# 局状态测试

局状态相关集成测试位于 `tests/round.rs`。

- `round_id_accepts_only_four_player_round_numbers` 验证局数只接受 `1..=4`，覆盖
  `0`、`5` 和 `u8::MAX`；
- `round_id_exposes_wind_and_derives_dealer_from_number` 覆盖四种场风和四个局数，
  验证字段读取及庄家推导；
- `round_phase_reports_a_player_only_when_the_variant_stores_one` 覆盖开局、摸牌后、
  弃牌后、鸣牌后、三种杠声明以及终局阶段，并验证开局和终局不重复保存玩家；
- `round_state_exposes_snapshot_fields` 验证快照保留玩家顺序、局信息、计数、宝牌
  指示牌顺序和当前阶段，并能按 `PlayerIndex` 读取玩家。

当前尚未覆盖事件导致的状态迁移、事件合法性、立直状态和连续多家和牌；这些行为
属于后续状态转移层及玩家状态扩展。
