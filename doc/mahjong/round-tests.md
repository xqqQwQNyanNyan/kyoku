# 局状态测试

局状态相关集成测试位于 `tests/round.rs`。

- `round_id_accepts_only_four_player_round_numbers` 验证局数只接受 `1..=4`，覆盖
  `0`、`5` 和 `u8::MAX`；
- `round_id_exposes_wind_and_derives_dealer_from_number` 覆盖四种场风和四个局数，
  验证字段读取及庄家推导；
- `round_phase_reports_a_player_only_when_the_variant_stores_one` 覆盖开局、摸牌后、
  弃牌后、鸣牌后、三种杠声明以及终局阶段，并验证开局和终局不重复保存玩家；
- `round_state_exposes_snapshot_fields` 验证快照保留玩家顺序、局信息、计数、宝牌
  指示牌顺序和当前阶段，并能按 `PlayerIndex` 读取玩家；
- `round_start_draw_and_discard_update_owned_state` 验证开局默认值，以及摸打对指定玩家、
  剩余摸牌数、牌河和局面阶段的更新；
- `draw_with_an_empty_wall_leaves_the_round_unchanged` 验证剩余摸牌为零时返回明确错误，
  并且手牌、阶段和计数均不改变。

当前尚未覆盖事件合法性、鸣牌、立直、和牌和流局。
