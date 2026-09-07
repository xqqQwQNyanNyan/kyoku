"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { createDownloader } = require("../downloader.cjs");

test("连接退出、超时后能重新启动，成功请求复用连接", async (t) => {
  const downloader = createDownloader({
    workerPath: require.resolve("./mock-worker.cjs"),
    timeoutMs: 500,
  });
  t.after(() => downloader.close());
  assert.deepEqual(await downloader.download("first"), { uuid: "first" });
  assert.deepEqual(await downloader.download("second"), { uuid: "second" });
  await assert.rejects(downloader.download("crash"), {
    code: "upstream_unavailable",
  });
  assert.deepEqual(await downloader.download("after-crash"), {
    uuid: "after-crash",
  });
  await assert.rejects(downloader.download("timeout"), {
    code: "upstream_timeout",
  });
  assert.deepEqual(await downloader.download("after-timeout"), {
    uuid: "after-timeout",
  });
});
