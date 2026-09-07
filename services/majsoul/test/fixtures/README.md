`ranked-round.json` 摘自 Fat-pig-Cui/misc-code 公开牌谱的东一局：
https://github.com/Fat-pig-Cui/misc-code/blob/main/paipu/paipu_210815-6da08e40-2605-42fb-a5e3-f8aa5940362a.json

保留该局的起手、摸打、鸣牌、立直和结算，删除账号资料、时间及与转换无关的操作信息。
`head.result` 使用占位分数，仅用于转换器的元数据结构；实际单局结算保留原始数据。
原仓库 MIT 许可见 LICENSE。

`ranked-round.tenhou.json` 是此样本的预期转换结果，昵称为占位符。
服务端测试将新旧 Protobuf 容器分别解码、转换，并与此文件对照；桌面测试继续验证回放。
此样本不覆盖杠或多家和牌，这些仍需后续真实样本补充。
