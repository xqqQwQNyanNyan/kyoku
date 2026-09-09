"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const MJSoul = require("mjsoul");
const { connect } = require("../client.cjs");

test("Unity 线路发现、平台握手和资源版本用于实际登录请求", async (t) => {
  const requests = [];
  const disconnects = [];
  const schema = {
    nested: {
      lq: {
        nested: {
          Wrapper: { fields: {} },
          ReqRequestConnection: {
            fields: {
              type: { type: "uint32", id: 2 },
              route_id: { type: "string", id: 3 },
              timestamp: { type: "uint64", id: 4 },
            },
          },
        },
      },
    },
  };
  t.mock.method(global, "fetch", async (input) => {
    const url = new URL(input);
    if (url.hostname === "offline.example") throw new Error("offline");
    const paths = {
      "/1/version.json": { version: "0.11.252.w" },
      "/1/resversion0.11.252.w.json": {
        res: { "res/proto/liqi.json": { prefix: "v0.11.243.w" } },
      },
      "/1/v0.11.243.w/res/proto/liqi.json": schema,
      "/1/v0.11.252.w/config.json": {
        ip: [
          {
            gateways: [
              { url: "https://offline.example" },
              { url: "https://route.example" },
            ],
          },
        ],
      },
      "/api/clientgate/routes": {
        data: {
          routes: [
            { id: "route-test", domain: "route.example:443", ssl: true },
          ],
        },
      },
    };
    assert.ok(paths[url.pathname], url.pathname);
    if (url.pathname === "/api/clientgate/routes") {
      assert.equal(url.searchParams.get("platform"), "Web");
    }
    return new Response(JSON.stringify(paths[url.pathname]));
  });
  t.mock.method(MJSoul.prototype, "open", function (done) {
    done();
  });
  t.mock.method(MJSoul.prototype, "sendAsync", async function (method, data) {
    requests.push({ service: this.service, method, data });
    if (method === "requestConnection") {
      const bytes = this.root
        .lookupType("ReqRequestConnection")
        .encode(data)
        .finish();
      assert.ok(
        Buffer.from(bytes).includes(Buffer.from([0x32, 3, 87, 101, 98])),
      );
    }
    return {};
  });
  const connection = await connect({
    base: "https://game.example/1",
    username: "test-account",
    password: "test-password",
    resourceVersion: "0.16.999",
  }, (code) => disconnects.push(code));
  assert.equal(connection.rpc.url, "wss://route.example:443/gateway");
  assert.equal(connection.clientVersion, "WebGL_2022-0.16.999");
  assert.deepEqual(
    requests.map(({ service, method }) => `${service}${method}`),
    [
      ".lq.Route.requestConnection",
      ".lq.Lobby.login",
      ".lq.Lobby.loginSuccess",
    ],
  );
  assert.equal(requests[0].data.route_id, "route-test");
  assert.equal(requests[1].data.client_version.resource, "0.16.999");
  assert.equal(
    requests[1].data.client_version_string,
    connection.clientVersion,
  );
  assert.match(requests[1].data.password, /^[0-9a-f]{64}$/);
  assert.equal(connection.rpc.service, ".lq.Lobby.");
  await assert.rejects(
    connect({
      base: "https://game.example/1",
      gateway: "wss://route.example/gateway",
    }),
    { code: "upstream_unavailable" },
  );
  assert.equal(requests.length, 3);
  connection.rpc.emit("error", new Error("private upstream details"));
  connection.rpc.emit("NotifyAccountLogout", { message: "private account" });
  connection.rpc.emit("close");
  assert.deepEqual(disconnects, [
    "gateway_failed",
    "account_logout",
    "connection_closed",
  ]);
});
