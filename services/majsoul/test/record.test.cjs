"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const pb = require("protobufjs");
const { convert } = require("../record.cjs");
const { decodeRecords } = require("../client.cjs");
const fixture = require("./fixtures/ranked-round.json");
const expected = require("./fixtures/ranked-round.tenhou.json");
const root = pb.Root.fromJSON(require("mjsoul/liqi.json"));
const wrapper = root.lookupType(".lq.Wrapper");

function recordBytes(record) {
  const type = root.lookupType(record.name);
  return wrapper
    .encode({
      name: record.name,
      data: type.encode(type.fromObject(record.data)).finish(),
    })
    .finish();
}
function encodedLog(version) {
  const records = fixture.records.map(recordBytes);
  const payload =
    version < 210715
      ? { version, records }
      : {
          version,
          actions: [{ type: 3 }, ...records.map((result) => ({ result }))],
        };
  return wrapper
    .encode({
      name: ".lq.GameDetailRecords",
      data: root.lookupType(".lq.GameDetailRecords").encode(payload).finish(),
    })
    .finish();
}

for (const version of [0, 210715]) {
  test(`真实单局的 Protobuf ${version} 容器转换保留赤牌、鸣牌、立直和结算`, () => {
    const data = decodeRecords({ root, wrapper }, encodedLog(version));
    assert.equal(data.length, fixture.records.length);
    const result = convert({ head: fixture.head, data });
    result.name = expected.name;
    assert.deepEqual(result, expected);
    assert.equal(result.log.length, 1);
    assert.deepEqual(result.log[0].at(-1)[1], [-12000, 0, 13000, 0]);
  });
}

test("三麻、友人房、活动规则和未知事件不被当作普通四麻转换", () => {
  const data = decodeRecords({ root, wrapper }, encodedLog(210715));
  for (const mutate of [
    (head) => head.result.players.pop(),
    (head) => {
      head.config.meta = { room_id: 123 };
    },
    (head) => {
      head.config.meta.mode_id = 46;
    },
    (head) => {
      head.config.mode.mode = 11;
    },
  ]) {
    const head = structuredClone(fixture.head);
    mutate(head);
    assert.throws(() => convert({ head, data }), { code: "unsupported_rules" });
  }
  assert.throws(() => convert({ head: fixture.head, data: [...data, {}] }), {
    code: "unsupported_record",
  });
});

test("双立直标记保留立直动作，未知鸣牌子类型明确报错", () => {
  const data = decodeRecords({ root, wrapper }, encodedLog(210715));
  const discard = data.find(
    (event) => event.constructor.name === "RecordDiscardTile" && event.is_liqi,
  );
  discard.is_liqi = false;
  discard.is_wliqi = true;
  const result = convert({ head: fixture.head, data });
  assert.deepEqual(result.log, expected.log);
  const call = data.find(
    (event) => event.constructor.name === "RecordChiPengGang",
  );
  call.type = 99;
  assert.throws(() => convert({ head: fixture.head, data }), {
    code: "unsupported_record",
  });
});

test("途中流局按协议原因转换，不依赖累计杠数或立直数", () => {
  const type = root.lookupType("RecordLiuJu");
  for (const [reason, expectedReason] of [
    [1, "九種九牌"],
    [2, "四風連打"],
    [3, "四開槓"],
    [4, "四家立直"],
  ]) {
    // 仅测试原因映射：起局后没有杠或立直，旧推断会把 3/4 错写为三家和。
    const start = decodeRecords({ root, wrapper }, encodedLog(210715))[0];
    const result = convert({
      head: fixture.head,
      data: [start, type.create({ type: reason })],
    });
    assert.deepEqual(result.log[0].at(-1), [expectedReason]);
  }
  for (const reason of [0, 5, 99]) {
    const start = decodeRecords({ root, wrapper }, encodedLog(210715))[0];
    assert.throws(
      () =>
        convert({
          head: fixture.head,
          data: [start, type.create({ type: reason })],
        }),
      {
        code: "unsupported_record",
      },
    );
  }
});
