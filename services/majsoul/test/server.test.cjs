"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { once } = require("node:events");
const { createServer } = require("../server.cjs");
const { parseId } = require("../record.cjs");

const ID = "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10";
const ANONYMOUS = "jijpmr-0415suwv-971c-67ei-ilom-qottvksmnvnn_a89702544_2";
async function start(t, download, options = {}) {
  const server = createServer({ download }, { cooldownMs: 0, ...options });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(() => {
    server.close();
    server.closeAllConnections();
  });
  return (path, init) =>
    fetch(`http://127.0.0.1:${server.address().port}${path}`, init);
}

test("普通、匿名、旧牌谱编号和玩家后缀", () => {
  assert.deepEqual(parseId(`${ID}_a89702544`), { uuid: ID, anonymous: false });
  assert.deepEqual(parseId(ANONYMOUS), { uuid: ID, anonymous: true });
  assert.deepEqual(parseId(`${ID}_2`), { uuid: ID, anonymous: false });
  assert.equal(parseId(ID.slice(7)).uuid, ID.slice(7));
  for (const bad of [
    "",
    "../secret",
    `${ID}_a`,
    `${ID}_a1_3`,
    `${ID}_a1_2_extra`,
    ANONYMOUS.replace("_2", ""),
  ]) {
    assert.throws(() => parseId(bad), { code: "invalid_id" });
  }
});

test("拒绝非法输入，不调用下载器", async (t) => {
  let calls = 0;
  const request = await start(t, () => {
    calls++;
  });
  for (const path of [
    "/convert",
    "/convert?id=../secret",
    `/convert?id=${ID}&id=${ID}`,
    `/convert?id=${ID}&url=https://example.com`,
  ]) {
    assert.equal((await request(path)).status, 400);
  }
  assert.equal((await request("/convert", { method: "POST" })).status, 405);
  assert.equal(calls, 0);
});

test("相同牌谱复用缓存，匿名输出隐藏昵称且与普通缓存分开", async (t) => {
  let calls = 0;
  const request = await start(t, async (uuid) => {
    assert.equal(uuid, ID);
    calls++;
    return { name: ["Alice", "Bob", "Carol", "Dave"], log: [] };
  });
  const first = await request(`/convert?id=${ID}_a1`);
  assert.equal(first.status, 200);
  assert.equal(first.headers.get("cache-control"), "no-store");
  assert.equal((await first.json()).name[0], "Alice");
  await request(`/convert?id=${ID}_a2`);
  assert.equal(calls, 1);
  const anonymous = await request(`/convert?id=${ANONYMOUS}`);
  assert.equal((await anonymous.json()).name[0], "玩家 1");
  const normal = await request(`/convert?id=${ID}`);
  assert.equal((await normal.json()).name[0], "Alice");
  assert.equal(calls, 2);
});

test("下载串行，忙碌时不积压请求，结束后应用冷却限制", async (t) => {
  let release;
  let started;
  const ready = new Promise((resolve) => {
    started = resolve;
  });
  const request = await start(
    t,
    () => {
      started();
      return new Promise((resolve) => {
        release = resolve;
      });
    },
    { cooldownMs: 10000 },
  );
  const first = request(`/convert?id=${ID}`);
  await ready;
  assert.equal((await request(`/convert?id=${ID}`)).status, 503);
  release({ name: [], log: [] });
  assert.equal((await first).status, 200);
  const other = await request(`/convert?id=${ID.slice(7)}`);
  assert.equal(other.status, 429);
  assert.equal(other.headers.get("retry-after"), "5");
});

test("上游失败不泄露错误详情，也不缓存失败结果", async (t) => {
  let calls = 0;
  const request = await start(t, async () => {
    calls++;
    throw new Error("ACCESS_TOKEN=secret");
  });
  for (let i = 0; i < 2; i++) {
    const response = await request(`/convert?id=${ID}`);
    assert.equal(response.status, 502);
    assert.deepEqual(await response.json(), {
      error: { code: "upstream_failed" },
    });
  }
  assert.equal(calls, 2);
});

test("缓存预算限制和响应大小限制", async (t) => {
  let calls = 0;
  const request = await start(
    t,
    async () => {
      calls++;
      return { log: ["test"] };
    },
    { cacheBytes: 0 },
  );
  await request(`/convert?id=${ID}`);
  await request(`/convert?id=${ID}`);
  assert.equal(calls, 2);
  const large = await start(t, async () => ({
    log: "x".repeat(16 * 1024 * 1024),
  }));
  assert.equal((await large(`/convert?id=${ID}`)).status, 413);
});
