"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");

async function runDesktop(mode) {
  // 在独立进程模拟依赖加载和连接事件，验证退出前确实送达脱敏错误。
  const program = `
    const Module = require('node:module');
    const original = Module._load;
    Module._load = function (name, ...args) {
      if (name === './client.cjs') {
        if (${JSON.stringify(mode)} === 'startup') throw new Error('private secret');
        return {
          connect: async (_, stop) => {
            if (${JSON.stringify(mode)} === 'crash') {
              setImmediate(() => { throw new Error('private secret'); });
            } else {
              stop(${JSON.stringify(mode)});
              stop('connection_closed');
            }
            await new Promise(() => {});
          }
        };
      }
      return original.call(this, name, ...args);
    };
    require(${JSON.stringify(require.resolve("../desktop.cjs"))});
  `;
  const child = spawn(process.execPath, ["-e", program], {
    stdio: ["pipe", "pipe", "pipe"],
    timeout: 5000,
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (data) => { stdout += data; });
  child.stderr.on("data", (data) => { stderr += data; });
  child.stdin.on("error", () => {});
  child.stdin.write(JSON.stringify({
    action: "login", username: "test", password: "secret", accept_risk: true,
  }) + "\n");
  const code = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", resolve);
  });
  assert.equal(code, 1);
  assert.equal(stderr, "");
  return stdout;
}

test("组件启动、崩溃和网关断线在退出前只上报一次白名单错误", async () => {
  for (const [mode, expected] of [
    ["startup", "component_start_failed"],
    ["crash", "component_failed"],
    ["gateway_failed", "gateway_failed"],
    ["account_logout", "account_logout"],
    ["heartbeat_failed", "heartbeat_failed"],
  ]) {
    assert.equal(await runDesktop(mode), JSON.stringify({ error: expected }) + "\n");
  }
});
