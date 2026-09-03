# 玩家索引测试

`PlayerIndex` 的集成测试位于 `tests/player_index.rs`。

- `constructor_accepts_only_four_player_indices` 验证 `0..=3` 全部合法，并覆盖
  紧邻上界的 `4` 和 `u8::MAX` 两个非法边界；
- `integer_conversion_reports_the_invalid_value` 验证 `TryFrom<u8>` 的成功路径、
  错误中的原始值和错误说明；
- `get_id_returns_the_original_value` 遍历四个合法索引，验证整数往返不变。

当前只表达四人麻将的绝对玩家索引，不表达相对方位。三人麻将等规则变体尚未
建模。
