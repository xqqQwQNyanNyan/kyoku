"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { createSession } = require("../desktop-session.cjs");
const fixture = require("./fixtures/ranked-round.tenhou.json");
const id = "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10";

test("未确认风险或凭据无效时不登录；只向固定官方地址发送账号密码", async () => {
  let logins = 0;
  const session = createSession({
    connect: async (config) => {
      logins++;
      assert.deepEqual(config, {
        base: "https://game.maj-soul.com/1",
        username: "test",
        password: "secret",
      });
      return {};
    },
    download: async () => structuredClone(fixture),
  });
  assert.deepEqual(await session({ action: "download", id }), {
    error: "login_required",
  });
  assert.deepEqual(
    await session({ action: "login", username: "test", password: "secret" }),
    { error: "risk_not_accepted" },
  );
  assert.deepEqual(
    await session({
      action: "login",
      username: "test",
      password: "",
      accept_risk: true,
    }),
    { error: "invalid_credentials" },
  );
  assert.equal(logins, 0);
  const login = {
    action: "login",
    username: " test ",
    password: "secret",
    accept_risk: true,
    base: "https://untrusted.example",
  };
  assert.deepEqual(await session(login), { logged_in: true });
  assert.equal(login.password, "");
  assert.equal(login.username, "");
  assert.equal(logins, 1);
  assert.deepEqual(await session({ action: "login" }), {
    error: "already_attempted",
  });
});

test("同一桌面会话复用连接和缓存，新牌谱遵守冷却", async () => {
  let downloads = 0;
  const connection = {};
  const session = createSession({
    connect: async () => connection,
    download: async (active, uuid) => {
      assert.equal(active, connection);
      assert.equal(uuid, id);
      downloads++;
      return structuredClone(fixture);
    },
  });
  await session({
    action: "login",
    username: "test",
    password: "secret",
    accept_risk: true,
  });
  assert.deepEqual((await session({ action: "download", id })).log, fixture);
  assert.deepEqual((await session({ action: "download", id })).log, fixture);
  assert.equal(downloads, 1);
  assert.deepEqual(
    await session({
      action: "download",
      id: "200516-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10",
    }),
    { error: "rate_limited" },
  );
  assert.deepEqual(await session({ action: "download", id: "../../secret" }), {
    error: "invalid_id",
  });
});

test("登录失败不返回上游内容，不保留凭据或自动重试", async () => {
  let attempts = 0;
  const session = createSession({
    connect: async () => {
      attempts++;
      throw { error: { code: 1002, message: "secret token" } };
    },
  });
  const request = {
    action: "login",
    username: "test",
    password: "secret",
    accept_risk: true,
  };
  assert.deepEqual(await session(request), { error: "login_failed" });
  assert.equal(request.password, "");
  assert.deepEqual(await session(request), { error: "already_attempted" });
  assert.deepEqual(await session({ action: "download", id }), {
    error: "login_required",
  });
  assert.equal(attempts, 1);
});
