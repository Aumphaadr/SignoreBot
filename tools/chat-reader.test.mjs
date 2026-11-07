import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import { runInNewContext } from 'node:vm';
const source = readFileSync(new URL('../src/components/Reader/reader.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
const helpers = {};
runInNewContext(outputText, { exports: helpers }, { filename: 'reader.js' });
const { tracked, forUs, nicknameColor, rewardNeedsInput, readerRewardItems } = helpers;
const prefs = { showEvents: true, ignoreOwnBot: true, ignoredLogins: ['StreamElements'] };
const entry = { userId: 'viewer', login: 'Viewer', event: '', deleted: false, sentByBot: false, reply: null, fragments: [] };
test('exclusions stay separate from read state and message visibility', () => {
  assert.equal(tracked(entry, prefs, 'bot', 'host'), true);
  assert.equal(tracked({ ...entry, login: 'streamelements' }, prefs, 'bot', 'host'), false);
  assert.equal(tracked({ ...entry, userId: 'bot' }, prefs, 'bot', 'host'), false);
  assert.equal(tracked({ ...entry, deleted: true }, prefs, 'bot', 'host'), false);
  assert.equal(tracked({ ...entry, event: 'Follow' }, { ...prefs, showEvents: false }, 'bot', 'host'), false);
});
test('shared-account manual messages count; known outbound echoes do not', () => {
  assert.equal(tracked({ ...entry, userId: 'host' }, prefs, 'host', 'host'), true);
  assert.equal(tracked({ ...entry, userId: 'host', sentByBot: true }, prefs, 'host', 'host'), false);
  assert.equal(tracked({ ...entry, sentByBot: true }, { ...prefs, ignoreOwnBot: false }, 'bot', 'host'), true);
});
test('our mentions and direct replies use streamer ID, never thread root or bot ID', () => {
  assert.equal(forUs({ ...entry, fragments: [{ userId: 'host', text: '@HOST' }] }, 'host'), true);
  assert.equal(forUs({ ...entry, fragments: [{ userId: 'bot', text: '@Bot' }] }, 'host'), false);
  assert.equal(forUs({ ...entry, reply: { userId: 'host' } }, 'host'), true);
  assert.equal(forUs({ ...entry, reply: { userId: 'someone', threadUserId: 'host' } }, 'host'), false);
  assert.equal(forUs({ ...entry, text: 'host', fragments: [] }, 'host'), false);
  assert.equal(forUs(entry, null), false);
});
test('nickname colors remain readable and reject arbitrary CSS', () => {
  assert.equal(nicknameColor('#FFFFFF'), '#FFFFFF');
  assert.equal(nicknameColor('#000000'), 'rgb(128,128,128)');
  assert.equal(nicknameColor('url(https://example.test)'), '#c3bafa');
});

test('reward input follows Twitch metadata and configured message variables', () => {
  const reward = (components = [], chatEnabled = true) => ({ response: { chat: { enabled: chatEnabled, components }, media: { enabled: false } } });
  assert.equal(rewardNeedsInput(reward(), { requiresInput: true }), true);
  assert.equal(rewardNeedsInput(reward()), false);
  assert.equal(rewardNeedsInput(reward([{ type: 'variable', name: 'message' }])), true);
  assert.equal(rewardNeedsInput(reward([{ type: 'static', value: 'Привет, {message}' }])), true);
  assert.equal(rewardNeedsInput(reward([{ type: 'phrase', phrases: ['{message}'] }])), true);
  assert.equal(rewardNeedsInput(reward([{ type: 'variable', name: 'message' }], false)), false);
  assert.equal(rewardNeedsInput({ response: { chat: { enabled: false }, media: { enabled: true, text: { enabled: true, content: '{message}' } } } }), true);
});

test('reward catalog includes unconfigured text rewards and sorts by ascending points', () => {
  const channel = [{ id: 'expensive', title: 'B', cost: 200, isEnabled: true }, { id: 'cheap', title: 'A', cost: 50, isEnabled: true }, { id: 'text', title: 'Question', cost: 100, requiresInput: true, isEnabled: true }];
  const reactions = [{ id: 'configured', rewardId: 'expensive', enabled: true }];
  const rows = readerRewardItems(channel, reactions);
  assert.deepEqual(Array.from(rows, (r) => r.id), ['cheap', 'text', 'expensive']);
  assert.equal(rows[1].reaction, undefined); assert.equal(rewardNeedsInput(rows[1].reaction, rows[1].channel), true);
  assert.equal(rows[2].reaction.id, 'configured'); assert.equal(channel[0].id, 'expensive');
});
test('unknown reward prices sort last and disabled Twitch rewards stay hidden', () => {
  const rows = readerRewardItems([{ id: 'off', title: 'Disabled', cost: 1, isEnabled: false }, { id: 'on', title: 'Enabled', cost: 50, isEnabled: true }], [{ id: 'off-response', rewardId: 'off', enabled: true }, { id: 'orphan', rewardId: 'missing', rewardTitle: 'Offline copy', enabled: true }]);
  assert.deepEqual(Array.from(rows, (r) => r.id), ['on', 'missing']);
});

test('an empty exclusion list does not silently exclude common bots', () => {
  const empty = { ...prefs, ignoredLogins: [] };
  for (const login of ['StreamElements', 'Nightbot', 'WizeBot', 'Moobot', 'JeetBot']) {
    assert.equal(tracked({ ...entry, login, userId: login }, empty, 'own-bot', 'host'), true);
  }
});
