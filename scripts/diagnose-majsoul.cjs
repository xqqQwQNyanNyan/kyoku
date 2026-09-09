"use strict";

const path = require("node:path");
const { spawn } = require("node:child_process");
const service = path.resolve(process.argv[2] || ".");
// 报告只由本脚本写入，第三方诊断文本不进入可分享的文件。
console.log = console.info = console.warn = console.error = console.debug = () => {};

function report(event, details = {}) {
  process.stdout.write(JSON.stringify({ event, ...details }) + "\n");
}

function safeError(error) {
  const code = error?.cause?.code || error?.code;
  const status = /^Unexpected server response: (\d{3})$/.exec(error?.message);
  // 只保留已知组件文件的行列号，不输出原始堆栈中的错误正文或本机路径。
  const files = new Set(["client.cjs", "mjsoul.js", "reader.js", "reader_buffer.js", "decoder.js", "record.cjs", "convert.js"]);
  const frames = typeof error?.stack === "string" ? error.stack.split("\n").slice(1).flatMap((line) => {
    const match = /[/\\]([\w.-]+):(\d+):(\d+)\)?$/.exec(line.trim());
    return match && files.has(match[1]) ? [`${match[1]}:${match[2]}:${match[3]}`] : [];
  }).slice(0, 5) : [];
  return {
    code: typeof code === "string" && /^[A-Z0-9_]{1,64}$/.test(code) ? code : "UNKNOWN",
    ...(["TypeError", "RangeError", "SyntaxError"].includes(error?.name) ? { type: error.name } : {}),
    ...(Number.isInteger(error?.error?.code) ? { rpcCode: error.error.code } : {}),
    ...(status ? { httpStatus: Number(status[1]) } : {}),
    ...(frames.length ? { frames } : {}),
  };
}

function finish(event, exitCode, details = {}) {
  process.stdout.write(JSON.stringify({ event, ...details }) + "\n", () => process.exit(exitCode));
}

async function readCredentials() {
  const { createInterface } = require("node:readline");
  const input = createInterface({ input: process.stdin, terminal: false });
  for await (const line of input) {
    if (Buffer.byteLength(line) > 8192) throw new Error("invalid input");
    const data = JSON.parse(line);
    if (data.accept_risk !== true || typeof data.username !== "string" ||
        !data.username.trim() || Buffer.byteLength(data.username) > 256 ||
        typeof data.password !== "string" || !data.password || Buffer.byteLength(data.password) > 1024)
      throw new Error("invalid credentials");
    return {
      username: data.username.trim(), password: data.password,
      ...(typeof data.replay === "string" ? { replay: data.replay } : {}),
    };
  }
  throw new Error("missing credentials");
}

async function probe(credentials) {
  let stage = "load_component";
  let done = false;
  const stop = (event, code, details = {}) => {
    if (done) return;
    done = true;
    finish(event, code, { stage, ...details });
  };
  setTimeout(() => stop("TIMEOUT", 1), 45000).unref();
  process.on("uncaughtException", (error) => stop("EXCEPTION", 1, safeError(error)));
  process.on("unhandledRejection", (error) => stop("REJECTION", 1, safeError(error)));
  try {
    const MJSoul = require(path.join(service, "node_modules/mjsoul"));
    const client = require(path.join(service, "client.cjs"));
    report("COMPONENT_OK");
    const originalEmit = MJSoul.prototype.emit;
    MJSoul.prototype.emit = function (...args) {
      // 诊断已经终止时，不让旧版退出监听器抢先截断输出。
      if (done) return true;
      if (args[0] === "NotifyAccountLogout") {
        stop("ACCOUNT_LOGOUT", 1);
        return true;
      }
      return originalEmit.apply(this, args);
    };
    const originalFetch = global.fetch;
    global.fetch = async (url, options) => {
      const pathname = new URL(url).pathname;
      const label = pathname.endsWith("/version.json") ? "version"
        : pathname.includes("/resversion") ? "resources"
        : pathname.endsWith("/liqi.json") ? "protocol"
        : pathname.endsWith("/config.json") ? "config" : "routes";
      stage = `https_${label}`;
      try {
        const response = await originalFetch(url, options);
        report("HTTP", { stage, status: response.status });
        return response;
      } catch (error) {
        report("HTTP_ERROR", { stage, ...safeError(error) });
        throw error;
      }
    };
    const originalOpen = MJSoul.prototype.open;
    MJSoul.prototype.open = function (...args) {
      stage = "websocket";
      report("WS_CONNECTING");
      const result = originalOpen.apply(this, args);
      // 在旧版 client 的 process.exit 监听器前取得底层错误，不输出响应正文。
      this.ws.prependListener("error", (error) => stop("WS_ERROR", 1, safeError(error)));
      this.ws.prependListener("close", (code) => stop("WS_CLOSED", 1, { closeCode: code }));
      this.ws.prependListener("open", () => report("WS_OPEN"));
      return result;
    };
    const allowed = new Set([".lq.Route.requestConnection"]);
    if (credentials) {
      allowed.add(".lq.Lobby.login");
      allowed.add(".lq.Lobby.loginSuccess");
      allowed.add(".lq.Route.heartbeat");
    }
    const sent = new Set();
    const originalSend = MJSoul.prototype.send;
    MJSoul.prototype.send = function (name, ...args) {
      // 默认只放行握手；显式登录模式最多发送一次登录，不自动重试。
      const method = this.service + name;
      if (!allowed.has(method) || (sent.has(method) && method !== ".lq.Route.heartbeat")) {
        stop("RPC_BLOCKED", 1);
        return;
      }
      sent.add(method);
      return originalSend.call(this, name, ...args);
    };
    const originalSendAsync = MJSoul.prototype.sendAsync;
    MJSoul.prototype.sendAsync = function (name, data) {
      if (!credentials && (name === "login" || name === "oauth2Login")) {
        stop("PRELOGIN_OK", 0);
        return new Promise(() => {});
      }
      const method = this.service + name;
      if (!allowed.has(method) || (sent.has(method) && method !== ".lq.Route.heartbeat")) {
        stop("RPC_BLOCKED", 1);
        return new Promise(() => {});
      }
      const phases = {
        requestConnection: ["route_handshake", "HANDSHAKE"],
        login: ["account_login", "LOGIN"],
        loginSuccess: ["login_success", "LOGIN_SUCCESS"],
        heartbeat: ["heartbeat", "HEARTBEAT"],
      };
      const [nextStage, label] = phases[name];
      stage = nextStage;
      report(label + "_START");
      return originalSendAsync.call(this, name, data).then((result) => {
        report(label + "_OK");
        return result;
      }, (error) => {
        stop("RPC_FAILED", 1, safeError(error));
        return new Promise(() => {});
      });
    };
    await client.connect({ base: "https://game.maj-soul.com/1", username: "", password: "", ...credentials });
    if (credentials) {
      credentials.username = "";
      credentials.password = "";
      stage = "session_watch";
      report("SESSION_WATCH");
      setTimeout(() => stop("LOGIN_SESSION_OK", 0), 6500);
    } else stop("UNEXPECTED_COMPLETION", 1);
  } catch (error) {
    stop("FAILED", 1, safeError(error));
  }
}

function cleanEnvironment() {
  const environment = {};
  const names = process.platform === "win32"
    ? ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "USERPROFILE"] : ["PATH"];
  for (const name of names) {
    if (process.env[name] !== undefined) environment[name] = process.env[name];
  }
  return environment;
}

async function main() {
  report("RUNTIME", { version: process.version, platform: process.platform, arch: process.arch });
  // 保持 stdin 打开，与桌面端私有管道一致；不发送任何登录请求。
  const smoke = spawn(process.execPath, [path.join(service, "desktop.cjs")], {
    env: cleanEnvironment(), stdio: ["pipe", "pipe", "ignore"], timeout: 10000,
  });
  const ready = await new Promise((resolve) => {
    let output = "";
    let complete = false;
    const end = (ok, details = {}) => {
      if (complete) return;
      complete = true;
      report(ok ? "PIPE_OK" : "PIPE_FAILED", details);
      smoke.kill();
      resolve(ok);
    };
    smoke.once("error", (error) => end(false, safeError(error)));
    smoke.once("close", (exitCode) => end(false, { exitCode }));
    smoke.stdin.on("error", () => {});
    smoke.stdout.on("data", (data) => {
      output += data.toString();
      if (output.length > 8192) return end(false);
      if (!output.includes("\n")) return;
      try { end(JSON.parse(output.split("\n")[0]).error === "login_required"); }
      catch { end(false); }
    });
    smoke.stdin.write('{"action":"download","id":"test"}\n');
  });
  if (!ready) { process.exitCode = 1; return; }
  const child = spawn(process.execPath, [__filename, service, "--probe"], {
    env: cleanEnvironment(), stdio: ["ignore", "inherit", "ignore"], timeout: 50000,
  });
  child.once("error", (error) => { report("PROBE_START_FAILED", safeError(error)); process.exitCode = 1; });
  child.once("close", (exitCode, signal) => {
    report("PROBE_EXIT", { exitCode, signal });
    process.exitCode = exitCode === 0 ? 0 : 1;
  });
}

async function desktopProbe() {
  const credentials = await readCredentials();
  let link = credentials.replay?.trim().replace(/^雀魂牌[谱譜]\s*[:：]\s*/u, "");
  if (!link || link.length > 1024) throw new Error("invalid replay");
  const url = new URL(link);
  if (!['http:', 'https:'].includes(url.protocol) || ![
    'game.maj-soul.com', 'game.maj-soul.net', 'game.majsoul.com',
    'game.mahjongsoul.com', 'mahjongsoul.game.yo-star.com', 'mahjongsoul.game.yo-star.net',
  ].includes(url.hostname) || url.username || url.password || url.port || url.searchParams.getAll('paipu').length !== 1)
    throw new Error("invalid replay");
  const id = url.searchParams.get('paipu');
  require(path.join(service, 'record.cjs')).parseId(id);
  delete credentials.replay;
  report("RUNTIME", { version: process.version, platform: process.platform, arch: process.arch });
  report("REPLAY_ID_OK");
  const child = spawn(process.execPath, [
    '--require', path.join(__dirname, 'trace-majsoul.cjs'), path.join(service, 'desktop.cjs'),
  ], { env: cleanEnvironment(), windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let phase = 'login';
  let complete = false;
  let stdout = '';
  let stderr = '';
  let timer;
  const end = (event, exitCode, details = {}) => {
    if (complete) return;
    complete = true;
    clearTimeout(timer);
    report(event, { phase, ...details });
    child.kill();
    process.exitCode = exitCode;
  };
  const send = (request) => {
    clearTimeout(timer);
    timer = setTimeout(() => end('PIPE_TIMEOUT', 1), 45000);
    child.stdin.write(JSON.stringify(request) + '\n', (error) => {
      if (error) end('PIPE_WRITE_FAILED', 1, safeError(error));
    });
  };
  const traceEvents = new Set(['HTTP', 'HTTP_ERROR', 'WS_OPEN', 'WS_ERROR', 'WS_CLOSED',
    'RPC_START', 'RPC_OK', 'RPC_FAILED', 'ACCOUNT_LOGOUT', 'EXCEPTION',
    'COMPONENT_EXIT', 'CONVERSION_START', 'CONVERSION_OK', 'DOWNLOAD_FAILED']);
  child.stderr.on('data', data => {
    stderr += data.toString('utf8');
    if (stderr.length > 8192) { stderr = ''; return; }
    const lines = stderr.split('\n');
    stderr = lines.pop();
    for (const line of lines) {
      try {
        const event = JSON.parse(line);
        if (traceEvents.has(event.event)) report(event.event, { ...event, phase });
      } catch { /* 原始第三方 stderr 不进入报告。 */ }
    }
  });
  const receive = line => {
    let response;
    try { response = JSON.parse(line); }
    catch { return end('PIPE_INVALID_JSON', 1); }
    if (!response || typeof response !== 'object' || Object.keys(response).some(k => !['error', 'logged_in', 'log'].includes(k)))
      return end('PIPE_INVALID_RESPONSE', 1);
    if (response.error) {
      const code = ['login_failed', 'client_outdated', 'download_failed', 'unsupported_rules',
        'unsupported_record', 'conversion_failed', 'invalid_id', 'login_required', 'log_too_large',
        'component_start_failed', 'component_failed', 'gateway_failed', 'connection_closed',
        'account_logout', 'heartbeat_failed'].includes(response.error) ? response.error : 'unknown';
      return end('DESKTOP_ERROR', 1, { code });
    }
    if (phase === 'login') {
      if (response.logged_in !== true || response.log != null) return end('PIPE_INVALID_RESPONSE', 1);
      report('PIPE_LOGIN_OK');
      phase = 'download';
      send({ action: 'download', id });
    } else {
      if (response.logged_in != null || !response.log || typeof response.log !== 'object' || Array.isArray(response.log))
        return end('PIPE_INVALID_RESPONSE', 1);
      end('PIPE_DOWNLOAD_OK', 0, { bytes: Buffer.byteLength(JSON.stringify(response.log)) });
    }
  };
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', data => {
    if (complete) return;
    stdout += data;
    if (Buffer.byteLength(stdout) > 16 * 1024 * 1024 + 1024) return end('PIPE_OUTPUT_TOO_LARGE', 1);
    let newline;
    while (!complete && (newline = stdout.indexOf('\n')) >= 0) {
      const line = stdout.slice(0, newline);
      stdout = stdout.slice(newline + 1);
      receive(line);
    }
  });
  child.stdin.on('error', () => {});
  child.once('error', error => end('COMPONENT_START_FAILED', 1, safeError(error)));
  child.once('close', (exitCode, signal) => end('PIPE_CLOSED', 1, { exitCode, signal }));
  send({ action: 'login', ...credentials, accept_risk: true });
  credentials.username = '';
  credentials.password = '';
}

module.exports = { safeError };
if (require.main === module && process.argv[3] === "--probe") probe();
else if (require.main === module && process.argv[3] === "--login-probe") {
  report("RUNTIME", { version: process.version, platform: process.platform, arch: process.arch });
  readCredentials().then(probe).catch(() => finish("INVALID_CREDENTIAL_INPUT", 1));
}
else if (require.main === module && process.argv[3] === '--desktop-probe') {
  desktopProbe().catch(() => finish('INVALID_DIAGNOSTIC_INPUT', 1));
}
else if (require.main === module) main().catch((error) => finish("DIAGNOSTIC_FAILED", 1, safeError(error)));
