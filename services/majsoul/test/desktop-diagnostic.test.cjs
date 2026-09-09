"use strict";
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const script = path.resolve(__dirname, '../../../scripts/diagnose-majsoul.cjs');
const id = '200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_a89702544';

test('原始桌面入口通过私有管道登录下载，区分转换、RPC、解码及协议错误', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kyoku 桌面 '));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'node_modules/mjsoul'), { recursive: true });
  for (const name of ['desktop.cjs', 'desktop-session.cjs', 'conversion.cjs'])
    fs.copyFileSync(path.join(__dirname, '..', name), path.join(root, name));
  fs.writeFileSync(path.join(root, 'record.cjs'), `
    const actual = require(${JSON.stringify(path.resolve(__dirname, '../record.cjs'))});
    module.exports = {...actual, convert: value => value};
  `);
  for (const mode of ['ok', 'rpc', 'conversion', 'decode', 'invalid_output']) {
    fs.writeFileSync(path.join(root, 'node_modules/mjsoul/index.js'), `
      const {EventEmitter} = require('node:events');
      module.exports = class M extends EventEmitter {
        open(callback) {
          this.ws = new EventEmitter();
          setImmediate(() => { this.ws.emit('open'); callback(); });
        }
        send(name, data, callback) {
          if (name === 'fetchGameRecord' && data.game_uuid !== '${id.split('_')[0]}') throw new Error('bad id');
          if (name === 'fetchGameRecord' && '${mode}' === 'decode') {
            setImmediate(() => { throw new RangeError('private token'); });
          } else callback(name === 'fetchGameRecord' && '${mode}' === 'rpc'
            ? {error:{code:1001, message:'private token'}} : {});
        }
        sendAsync(name, data) {
          return new Promise((resolve, reject) => this.send(name, data, r => r.error ? reject(r) : resolve(r)));
        }
      };
    `);
    fs.writeFileSync(path.join(root, 'client.cjs'), `
      const M = require('mjsoul');
      const {convert, ServiceError} = require('./record.cjs');
      exports.connect = async config => {
        if (config.username !== 'test-user' || config.password !== 'private token') throw new Error('bad credentials');
        const rpc = new M();
        await new Promise(resolve => rpc.open(resolve));
        rpc.service = '.lq.Route.';
        await rpc.sendAsync('requestConnection', {});
        rpc.service = '.lq.Lobby.';
        await rpc.sendAsync('login', config);
        await rpc.sendAsync('loginSuccess', {});
        return {rpc};
      };
      exports.download = async ({rpc}, id) => {
        await rpc.sendAsync('fetchGameRecord', {game_uuid:id});
        if ('${mode}' === 'conversion') throw new ServiceError('unsupported_rules', 422);
        if ('${mode}' === 'invalid_output') process.stdout.write('private token\\n');
        return convert(require(${JSON.stringify(path.resolve(__dirname, 'fixtures/ranked-round.tenhou.json'))}));
      };
    `);
    const result = spawnSync(process.execPath, [script, root, '--desktop-probe'], {
      encoding: 'utf8', timeout: 5000,
      input: JSON.stringify({username:'test-user', password:'private token', accept_risk:true,
        replay:'雀魂牌谱:https://game.maj-soul.com/1/?paipu=' + id}) + '\n',
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, mode === 'ok' ? 0 : 1, result.stdout);
    assert.equal(result.stderr, '');
    assert.ok(!result.stdout.includes('private token'));
    assert.ok(!result.stdout.includes('test-user'));
    assert.ok(!result.stdout.includes(id));
    const events = result.stdout.trim().split('\n').map(JSON.parse);
    assert.ok(events.some(e => e.event === 'PIPE_LOGIN_OK'), result.stdout);
    if (mode === 'ok') {
      assert.ok(events.some(e => e.event === 'CONVERSION_OK'), result.stdout);
      assert.ok(events.some(e => e.event === 'PIPE_DOWNLOAD_OK' && e.bytes > 0), result.stdout);
    } else if (mode === 'rpc') {
      assert.ok(events.some(e => e.event === 'RPC_FAILED' && e.stage === 'fetch_record' && e.rpcCode === 1001), result.stdout);
    } else if (mode === 'conversion') {
      assert.ok(events.some(e => e.event === 'DESKTOP_ERROR' && e.code === 'unsupported_rules'), result.stdout);
    } else if (mode === 'decode') {
      assert.ok(events.some(e => e.event === 'EXCEPTION' && e.type === 'RangeError'), result.stdout);
    } else {
      assert.ok(events.some(e => e.event === 'PIPE_INVALID_JSON'), result.stdout);
    }
  }
});
