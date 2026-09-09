"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const script = path.resolve(__dirname, "../../../scripts/diagnose-majsoul.cjs");

test("诊断在独立管道测试后停止于登录前，并保留脱敏网络和握手错误", (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "kyoku 诊断 "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, "node_modules/mjsoul"), { recursive: true });
  for (const mode of ["ok", "websocket", "closed", "handshake", "blocked", "pipe"]) {
    fs.writeFileSync(path.join(root, "desktop.cjs"), mode === "pipe" ? "process.exit(42)" : `
      require('node:readline').createInterface({input:process.stdin}).on('line', line => {
        if (JSON.parse(line).action !== 'download' || process.env.MJS_PASSWORD) process.exit(2);
        process.stdout.write('{"error":"login_required"}\\n');
      });
    `);
    fs.writeFileSync(path.join(root, "node_modules/mjsoul/index.js"), `
      const {EventEmitter} = require('node:events');
      module.exports = class M extends EventEmitter {
        open(callback) {
          this.ws = new EventEmitter();
          this.ws.on('error', e => this.emit('error', e));
          this.ws.on('close', () => this.emit('close'));
          setImmediate(() => {
            if ('${mode}' === 'websocket') this.ws.emit('error', Object.assign(new Error('private token'), {code:'ECONNRESET'}));
            else if ('${mode}' === 'closed') this.ws.emit('close', 1006);
            else { this.ws.emit('open'); callback(); }
          });
        }
        send(name) {
          if (name !== 'requestConnection') require('node:fs').writeFileSync(${JSON.stringify(path.join(root, "unsafe"))}, name);
        }
        async sendAsync(name, data) {
          this.send(name, data);
          if ('${mode}' === 'handshake') throw {error:{code:151, message:'private token'}};
          return {};
        }
      };
    `);
    fs.writeFileSync(path.join(root, "client.cjs"), `
      const M = require('mjsoul');
      exports.connect = async config => {
        if (config.username || config.password || process.env.MJS_PASSWORD) throw new Error('unexpected credentials');
        const rpc = new M();
        rpc.on('error', () => process.exit(1));
        rpc.on('close', () => process.exit(1));
        await new Promise(resolve => rpc.open(resolve));
        rpc.service = '.lq.Route.';
        await rpc.sendAsync('requestConnection', {});
        rpc.service = '.lq.Lobby.';
        await rpc.sendAsync('${mode === "blocked" ? "unexpected" : "login"}', {password:'private token'});
      };
    `);
    const result = spawnSync(process.execPath, [script, root], {
      encoding: "utf8", timeout: 15000,
      env: { ...process.env, MJS_PASSWORD: "private token" },
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, mode === "ok" ? 0 : 1, result.stdout);
    assert.equal(result.stderr, "");
    assert.ok(!result.stdout.includes("private token"));
    assert.ok(!fs.existsSync(path.join(root, "unsafe")), "不得发送账号登录或未知 RPC");
    const events = result.stdout.trim().split("\n").map(JSON.parse);
    const expected = {
      ok: "PRELOGIN_OK", websocket: "WS_ERROR", closed: "WS_CLOSED",
      handshake: "RPC_FAILED", blocked: "RPC_BLOCKED", pipe: "PIPE_FAILED",
    }[mode];
    const event = events.find(e => e.event === expected);
    assert.ok(event, result.stdout);
    if (mode === "websocket") assert.equal(event.code, "ECONNRESET");
    if (mode === "closed") assert.equal(event.closeCode, 1006);
    if (mode === "handshake") assert.equal(event.rpcCode, 151);
    if (mode === "pipe") assert.equal(event.exitCode, 42);
  }
});

test("显式诊断登录只从私有输入读取凭据，区分登录、确认和心跳失败且不重试", (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "kyoku login "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, "node_modules/mjsoul"), { recursive: true });
  const callsPath = path.join(root, "calls");
  for (const mode of ["login_error", "decode_error", "login_close", "logout", "confirm_error", "heartbeat_error", "ok", "retry"]) {
    fs.writeFileSync(callsPath, "");
    fs.writeFileSync(path.join(root, "node_modules/mjsoul/index.js"), `
      const {EventEmitter} = require('node:events');
      module.exports = class M extends EventEmitter {
        open(callback) {
          this.ws = new EventEmitter();
          this.ws.on('error', e => this.emit('error', e));
          this.ws.on('close', () => this.emit('close'));
          setImmediate(() => { this.ws.emit('open'); callback(); });
        }
        send(name) {
          require('node:fs').appendFileSync(${JSON.stringify(callsPath)}, this.service + name + '\\n');
        }
        async sendAsync(name, data) {
          this.send(name, data);
          if (name === 'login') {
            if ('${mode}' === 'login_error') throw {error:{code:151,message:'private token'}};
            if ('${mode}' === 'decode_error') {
              const error = new RangeError('private token');
              error.stack = 'RangeError: private token\\n at read (/Users/private token/reader.js:55:12)';
              throw error;
            }
            if ('${mode}' === 'login_close' || '${mode}' === 'logout') {
              queueMicrotask(() => '${mode}' === 'logout'
                ? this.emit('NotifyAccountLogout', {message:'private token'})
                : this.ws.emit('close', 1006));
              return new Promise(() => {});
            }
          }
          if (name === 'loginSuccess' && '${mode}' === 'confirm_error') throw {error:{code:1002,message:'private token'}};
          if (name === 'heartbeat' && '${mode}' === 'heartbeat_error') throw {error:{code:9997,message:'private token'}};
          return {access_token:'private token',account:'测试用户'};
        }
      };
    `);
    fs.writeFileSync(path.join(root, "client.cjs"), `
      const M = require('mjsoul');
      const timer = global.setTimeout;
      global.setTimeout = (callback, delay, ...args) => timer(callback, delay === 6500 ? 30 : delay, ...args);
      exports.connect = async config => {
        if (config.username !== '测试用户' || config.password !== 'private token') throw new Error('missing credentials');
        if (process.argv.some(v => v.includes('private token')) || Object.values(process.env).some(v => v.includes('private token')))
          throw new Error('credentials exposed');
        const rpc = new M();
        rpc.on('error', () => process.exit(1));
        rpc.on('close', () => process.exit(1));
        rpc.on('NotifyAccountLogout', () => process.exit(1));
        await new Promise(resolve => rpc.open(resolve));
        rpc.service = '.lq.Route.';
        await rpc.sendAsync('requestConnection', {});
        rpc.service = '.lq.Lobby.';
        await rpc.sendAsync('login', config);
        if ('${mode}' === 'retry') await rpc.sendAsync('login', config);
        await rpc.sendAsync('loginSuccess', {});
        timer(() => {
          rpc.service = '.lq.Route.';
          rpc.sendAsync('heartbeat', {}).catch(() => process.exit(1));
        }, 1);
        return {rpc};
      };
    `);
    const result = spawnSync(process.execPath, [script, root, "--login-probe"], {
      encoding: "utf8", timeout: 5000,
      input: JSON.stringify({ username: "测试用户", password: "private token", accept_risk: true }) + "\n",
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, mode === "ok" ? 0 : 1, result.stdout);
    assert.equal(result.stderr, "");
    assert.ok(!result.stdout.includes("private token"));
    assert.ok(!result.stdout.includes("测试用户"));
    const calls = fs.readFileSync(callsPath, "utf8").trim().split("\n");
    assert.equal(calls.filter(name => name === ".lq.Lobby.login").length, 1);
    const events = result.stdout.trim().split("\n").map(JSON.parse);
    if (mode === "ok") assert.ok(events.some(e => e.event === "LOGIN_SESSION_OK"));
    else if (mode === "retry") assert.ok(events.some(e => e.event === "RPC_BLOCKED"));
    else {
      const [event, stage, rpcCode] = {
        login_error: ["RPC_FAILED", "account_login", 151],
        decode_error: ["RPC_FAILED", "account_login"],
        login_close: ["WS_CLOSED", "account_login"],
        logout: ["ACCOUNT_LOGOUT", "account_login"],
        confirm_error: ["RPC_FAILED", "login_success", 1002],
        heartbeat_error: ["RPC_FAILED", "heartbeat", 9997],
      }[mode];
      assert.ok(events.some(e => e.event === event && e.stage === stage && e.rpcCode === rpcCode), result.stdout);
      if (mode === "decode_error") {
        const failure = events.find(e => e.event === "RPC_FAILED");
        assert.equal(failure.type, "RangeError");
        assert.deepEqual(failure.frames, ["reader.js:55:12"]);
      }
    }
  }
  for (const input of ["", "invalid", '{"username":"test","password":"private token"}\n']) {
    const result = spawnSync(process.execPath, [script, root, "--login-probe"], { encoding: "utf8", input });
    assert.equal(result.status, 1);
    assert.ok(result.stdout.includes("INVALID_CREDENTIAL_INPUT"));
    assert.ok(!result.stdout.includes("private token"));
  }
});
