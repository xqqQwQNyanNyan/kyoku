"use strict";

const { randomUUID, createHmac } = require("node:crypto");
const pb = require("protobufjs");
const MJSoul = require("mjsoul");
const { ServiceError, convert } = require("./record.cjs");
const MAX_BYTES = 16 * 1024 * 1024;

async function fetchBytes(url) {
  const target = new URL(url);
  if (target.protocol !== "https:")
    throw new ServiceError("upstream_failed", 502);
  const response = await fetch(target, {
    signal: AbortSignal.timeout(10000),
    redirect: "error",
  });
  if (!response.ok) throw new ServiceError("upstream_failed", 502);
  const chunks = [];
  let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > MAX_BYTES) throw new ServiceError("log_too_large", 413);
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}
async function fetchJson(url) {
  return JSON.parse((await fetchBytes(url)).toString("utf8"));
}

function routeRequest(rpc, method, data) {
  rpc.service = ".lq.Route.";
  try {
    return rpc.sendAsync(method, data);
  } finally {
    rpc.service = ".lq.Lobby.";
  }
}

// 旧版公开协议仍可解码牌谱；Unity 网关还需要平台字段。
async function connect(config, onDisconnect = () => process.exit(1)) {
  const base = config.base.replace(/\/$/, "");
  const { version } = await fetchJson(`${base}/version.json`);
  const resources = await fetchJson(`${base}/resversion${version}.json`);
  const prefix = resources.res["res/proto/liqi.json"].prefix;
  const root = pb.Root.fromJSON(
    await fetchJson(`${base}/${prefix}/res/proto/liqi.json`),
  );
  let gateway = config.gateway;
  let routeId = config.routeId;
  const discovery = await fetchJson(`${base}/v${version}/config.json`);
  const region = discovery.ip[0];
  const unity = Boolean(region.gateways?.length);
  if (gateway && unity && !routeId)
    throw new ServiceError("upstream_unavailable", 503);
  if (!gateway) {
    for (const entry of region.gateways || region.region_urls || []) {
      try {
        const url = unity
          ? new URL("/api/clientgate/routes", entry.url)
          : new URL(entry.url);
        if (unity) {
          url.searchParams.set("platform", "Web");
          url.searchParams.set("version", version);
        } else {
          url.searchParams.set("protocol", "ws");
          url.searchParams.set("ssl", "true");
          url.searchParams.set("service", "ws-gateway");
        }
        const endpoints = await fetchJson(url);
        if (unity) {
          const route = endpoints.data?.routes?.find(
            (r) => r.ssl && r.domain && r.id,
          );
          if (route) {
            gateway = `wss://${route.domain}/gateway`;
            routeId = route.id;
            break;
          }
        }
        if (endpoints.servers?.length) {
          gateway = `wss://${endpoints.servers[0]}/gateway`;
          break;
        }
      } catch {
        /* 当前线路不可用时尝试下一条官方线路。 */
      }
    }
  }
  if (!gateway || new URL(gateway).protocol !== "wss:")
    throw new ServiceError("upstream_unavailable", 503);
  const rpc = new MJSoul({
    url: gateway,
    root,
    wrapper: root.lookupType("Wrapper"),
    timeout: 10000,
    wsOption: {
      origin: new URL(base).origin,
      maxPayload: MAX_BYTES,
      handshakeTimeout: 10000,
    },
  });
  // 连接失效后结束工作进程，父进程在下一次导入时重新发现线路并登录。
  rpc.on("error", () => onDisconnect("gateway_failed"));
  rpc.on("close", () => onDisconnect("connection_closed"));
  rpc.on("NotifyAccountLogout", () => onDisconnect("account_logout"));
  await new Promise((resolve) => rpc.open(resolve));
  if (unity) {
    const request = root.lookupType("ReqRequestConnection");
    if (!request.fieldsById[6])
      request.add(new pb.Field("platform", 6, "string"));
    await routeRequest(rpc, "requestConnection", {
      type: 1,
      route_id: routeId,
      timestamp: Math.floor(Date.now() / 1000),
      [request.fieldsById[6].name]: "Web",
    });
  }
  // Unity 的 version.json 停留在旧版；此值来自官方登录画面（2026-09-07）。
  const resourceVersion = unity
    ? config.resourceVersion || "0.16.274"
    : version;
  const clientVersion = unity
    ? `WebGL_2022-${resourceVersion}`
    : `web-${version.replace(/\.w$/, "")}`;
  const method = config.token ? "oauth2Login" : "login";
  const credentials = config.token
    ? { type: config.loginType, access_token: config.token }
    : {
        account: config.username,
        password: createHmac("sha256", "lailai")
          .update(config.password)
          .digest("hex"),
        gen_access_token: true,
        currency_platforms: [1, 2, 5, 6, 8, 10, 11],
      };
  await rpc.sendAsync(method, {
    ...credentials,
    client_version_string: clientVersion,
    client_version: {
      resource: resourceVersion,
      package: unity ? "4.0.46" : "",
    },
    device: {
      hardware: "pc",
      is_browser: true,
      os: "mac",
      platform: "pc",
      sale_platform: "web",
      software: "Chrome",
    },
    random_key: randomUUID(),
    reconnect: false,
  });
  await rpc.sendAsync("loginSuccess", {});
  if (unity) {
    const heartbeat = setInterval(() => {
      routeRequest(rpc, "heartbeat", { platform: 11 }).catch(() =>
        onDisconnect("heartbeat_failed"),
      );
    }, 4000);
    heartbeat.unref();
    rpc.once("close", () => clearInterval(heartbeat));
  }
  return { rpc, clientVersion };
}

function decodeRecords(rpc, bytes) {
  const wrapper = rpc.wrapper.decode(bytes);
  if (wrapper.name !== ".lq.GameDetailRecords")
    throw new ServiceError("unsupported_record", 422);
  const payload = rpc.root.lookupType(wrapper.name).decode(wrapper.data);
  const actions =
    payload.version < 210715 && payload.records.length
      ? payload.records
      : payload.actions.filter((a) => a.result?.length).map((a) => a.result);
  return actions.map((bytes) => {
    const event = rpc.wrapper.decode(bytes);
    return rpc.root.lookupType(event.name).decode(event.data);
  });
}

async function download(connection, uuid) {
  const { rpc, clientVersion } = connection;
  const record = await rpc.sendAsync("fetchGameRecord", {
    game_uuid: uuid,
    client_version_string: clientVersion,
  });
  const bytes = record.data_url
    ? await fetchBytes(record.data_url)
    : record.data;
  record.data = decodeRecords(rpc, bytes);
  return convert(record);
}
module.exports = { connect, download, decodeRecords };
