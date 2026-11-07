// Real browser layout/input checks against the built app. Twitch IPC is simulated, not live.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir, rm } from 'node:fs/promises';
import { resolve, extname } from 'node:path';
import assert from 'node:assert/strict';
const out = process.env.SIGNOREBOT_READER_CHECK_DIR ?? '/tmp/signorebot-reader-check';
await mkdir(out, { recursive: true });
const dist = resolve('dist');
const server = createServer(async (req, res) => {
  try {
    const path = resolve(dist, '.' + new URL(req.url, 'http://localhost').pathname);
    if (!path.startsWith(dist + '/') && path !== dist) { res.writeHead(403).end(); return; }
    const file = path === dist + '/' || path === dist ? dist + '/index.html' : path;
    const data = await readFile(file);
    res.setHeader('content-type', ({ '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.ttf': 'font/ttf' })[extname(file)] ?? 'application/octet-stream'); res.end(data);
  } catch { res.writeHead(404).end(); }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const profile = out + '/chrome'; await mkdir(profile, { recursive: true }); await rm(profile + '/DevToolsActivePort', { force: true });
const chrome = spawn(process.env.CHROME ?? '/usr/bin/google-chrome', ['--headless', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', '--no-first-run', '--disable-background-networking', '--remote-debugging-port=0', '--window-size=1440,1000', '--user-data-dir=' + profile, 'about:blank'], { stdio: 'ignore' });
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
let ws;
const checks = [];
try {
  let port;
  for (let i = 0; i < 100; i++) { try { port = (await readFile(profile + '/DevToolsActivePort', 'utf8')).split('\n')[0]; break; } catch { await delay(50); } }
  if (!port) throw Error('Chrome did not start');
  const tabs = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  ws = new WebSocket(tabs.find((t) => t.type === 'page').webSocketDebuggerUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let seq = 0; const pending = new Map();
  ws.onmessage = (ev) => { const msg = JSON.parse(ev.data); const cb = pending.get(msg.id); if (cb) { pending.delete(msg.id); msg.error ? cb[1](msg.error) : cb[0](msg.result); } };
  const call = (method, params = {}) => new Promise((r, j) => { const id = ++seq; pending.set(id, [r, j]); ws.send(JSON.stringify({ id, method, params })); });
  const evaluate = async (expression) => { const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails)); return r.result.value; };
  await call('Page.enable');
  await call('Page.addScriptToEvaluateOnNewDocument', { source: `
    localStorage.setItem('signorebot.tab','reader');
    window.calls=[]; window.callbacks=new Map(); let next=0; const listeners=new Map();
    const p={mode:'read',autoScroll:true,tracking:true,badges:true,timestamps:false,fontSize:19,highlightOwn:true,showEvents:true,ignoreOwnBot:true,ignoredLogins:[]};
    window.cfg={version:2,chatReader:p,mediaSets:[],rewards:[{id:'r',rewardId:'rw',rewardTitle:'Приветствие',enabled:true,response:{chat:{enabled:true,components:[]},media:{enabled:false}}}],app:{uiZoom:100,notificationSeconds:3,notificationsSticky:false},notes:[],updates:{enabled:false},commands:[],accounts:{sameAccount:false}};
    const rewardNames=['Приветствие','Показать мем','Музыкальная пауза','Танец','Тишина','Выбрать игру'];
    cfg.rewards=rewardNames.map((title,i)=>({id:'r'+i,rewardId:'rw'+i,rewardTitle:title,enabled:true,response:{chat:{enabled:true,components:[]},media:{enabled:false}}}));
    const image='data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path fill="white" d="m32 4 8 17 19 3-14 13 3 19-16-9-16 9 3-19L5 24l19-3z"/></svg>');
    window.channelRewards=cfg.rewards.map((r,i)=>({id:r.rewardId,title:r.rewardTitle,cost:[900,300,1000,200,800,500][i],isEnabled:true,isPaused:false,requiresInput:i===0,backgroundColor:['#9146ff','#1d7c78','#bc4777','#4663ac','#e4c36a','#dd673b'][i],image,prompt:i===0?'Кого поприветствовать?':'',skipQueue:false,cooldownSeconds:null,maxPerStream:null,maxPerUserPerStream:null,isManaged:true}));
    const account=(login,id)=>({state:'authorized',login,userId:id,scopes:['moderator:manage:chat_messages','moderator:manage:banned_users','moderator:manage:warnings','user:write:chat'],missingScopes:[],expiresAt:0,error:null,device:null,attempt:null});
    channelRewards.push({...channelRewards[0],id:'rw-text',title:'Вопрос без настроенной реакции',cost:50,requiresInput:true,prompt:'Задайте вопрос'});
    window.st={broadcaster:account('host','host'),bot:account('bot','bot'),running:true,eventsub:{connected:true},authSetup:{sameAccount:false},server:{},overlays:[],migration:null,version:'1.0.7-dev',overlayAlert:null,obsProblem:null,update:null};
    window.row=(id,extra={})=>({id:'chat:'+id,messageId:id,timestamp:1794045600000,userId:'u'+id,login:'viewer'+id,name:'Viewer'+id,color:'#9090e0',text:'Сообщение '+id,fragments:[],badges:[],reply:null,event:'',rewardId:'',isReward:false,deleted:false,read:false,sentByBot:false,...extra});
    window.hist={channelId:'host',generation:1,revision:1,entries:Array.from({length:70},(_,i)=>row(String(i)))};
    hist.entries[63]=row('63',{isReward:true,rewardId:'rw0',event:'Награда · Приветствие',text:'Текст от зрителя'});
    hist.entries[64]=row('64',{isReward:true,rewardId:'rw1',event:'Награда · Показать мем',text:'',messageId:''});
    hist.entries[65]=row('65',{text:'@host привет',fragments:[{text:'@host',userId:'host',imageUrl:null},{text:' привет',userId:null,imageUrl:null}]});
    hist.entries[66]=row('66',{text:'@Someone привет',fragments:[{text:'@Someone',userId:'someone',imageUrl:null},{text:' привет',userId:null,imageUrl:null}]});
    hist.entries[67]=row('67',{reply:{messageId:'parent',userId:'host',name:'Host',text:'Родительская реплика',deleted:false}});
    hist.entries[68]=row('68',{userId:'bot',login:'bot',name:'Bot'});
    hist.entries[69]=row('69',{userId:'se',login:'StreamElements',name:'StreamElements'});
    window.emit=()=>{for(const {event,handler} of listeners.values())if(event==='changed')callbacks.get(handler)?.({event:'changed',payload:'reader',id:0});};
    window.add=()=>{hist.entries.push(row('new'+hist.entries.length));hist.revision++;emit();};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
    window.__TAURI_INTERNALS__={metadata:{currentWebview:{label:'main'},currentWindow:{label:'main'}},transformCallback:(fn)=>{callbacks.set(++next,fn);return next;},unregisterCallback:(id)=>callbacks.delete(id),invoke:async(cmd,args={})=>{
      calls.push({cmd,args});
      if(cmd==='config_get')return structuredClone(cfg);
      if(cmd==='status_get')return structuredClone(st);
      if(cmd==='chat_history')return structuredClone(hist);
      if(cmd==='chat_badges')return [];
      if(cmd==='rewards_channel'){if(window.rejectRewards)throw 'Twitch: каталог недоступен';return structuredClone(channelRewards);}
      if(cmd==='config_set_section'){cfg[args.section]=args.value;return structuredClone(cfg);}
      if(cmd==='chat_mark'){for(const e of hist.entries)if(args.ids.includes(e.id))e.read=args.read;hist.revision++;emit();return;}
      if(cmd==='chat_moderate'){if(window.rejectAction)throw 'Twitch: нет прав';return;}
      if(cmd==='response_test')return {chatSent:true,mediaSent:false,mediaUnavailable:false};
      if(cmd==='plugin:event|listen'){listeners.set(++next,args);return next;}
      if(cmd==='plugin:event|unlisten'){listeners.delete(args.eventId);return;}
    }};
  ` });
  await call('Page.navigate', { url: `http://127.0.0.1:${server.address().port}/` });
  for (let i = 0; i < 60; i++) { if (await evaluate('!!document.querySelector(".reader-row")')) break; await delay(100); }
  const check = async (name, expr) => { assert.equal(await evaluate(expr), true, name); checks.push(name); };
  const click = (label) => evaluate(`Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()===${JSON.stringify(label)}).click()`);
  const screenshot = async (name) => { await evaluate("document.querySelectorAll('.notification-close:not([title])').forEach(b=>b.click())"); await delay(250); const r = await call('Page.captureScreenshot', { format: 'png' }); await writeFile(out + '/' + name + '.png', Buffer.from(r.data, 'base64')); };
  await check('sidebar placement', `(()=>{const names=[...document.querySelectorAll('.sidebar-btn-label')].map(e=>e.textContent);return names.indexOf('Читать чат')===names.indexOf('Медиа')+1&&names.indexOf('Заметки')===names.indexOf('Читать чат')+1})()`);
  await check('streamer mentions/replies differ from other mentions', `document.querySelectorAll('.reader-row.for-us').length===2&&document.querySelectorAll('.reader-mention.own').length===1&&document.querySelectorAll('.reader-mention:not(.own)').length===1`);
  await check('external bots count as unread until explicitly excluded', `document.querySelectorAll('.reader-row').length===70&&document.querySelector('.reader-toolbar strong').textContent==='69'`);
  await check('real reader uses viewport without outer page scroll', `(()=>{const e=document.querySelector('.main-content');return e.scrollHeight<=e.clientHeight+1})()`);
  await screenshot('01-reading');
  await check('modes and autoscroll capsule share one row', `(()=>{const m=document.querySelector('.reader-modes').getBoundingClientRect(),t=document.querySelector('.reader-modebar .toggle-switch').getBoundingClientRect();return Math.abs(m.top+m.height/2-t.top-t.height/2)<=1&&t.left>m.right})()`);
  await check('autoscroll uses the shared capsule', `document.querySelector('.reader-modebar .toggle-slider')!==null&&!document.querySelector('select[aria-label="Прокрутка чата"]')`);
  await check('composer names the streamer', `document.querySelector('.reader-compose input').placeholder==='Написать от host'`);
  await evaluate(`{const el=document.querySelector('.reader-compose input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,'Привет от стримера');el.dispatchEvent(new Event('input',{bubbles:true}))}`);await delay(20);
  await evaluate(`document.querySelector('.reader-compose button').click()`);await delay(80);
  await check('composer uses streamer IPC and clears after success', `calls.some(c=>c.cmd==='chat_reader_send'&&c.args.text==='Привет от стримера')&&!calls.some(c=>c.cmd==='chat_send')&&document.querySelector('.reader-compose input').value===''`);

  const point = await evaluate(`(()=>{const r=document.querySelector('.reader-feed').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`);
  await call('Input.dispatchMouseEvent', { type: 'mouseWheel', ...point, deltaY: -550, deltaX: 0 }); await delay(250);
  const top = await evaluate('document.querySelector(".reader-feed").scrollTop');
  await evaluate('add()'); await delay(220);
  assert.equal(await evaluate('document.querySelector(".reader-feed").scrollTop'), top, 'new message preserves paused position'); checks.push('real wheel pauses and new arrivals keep position');
  const beforeRead = await evaluate('hist.entries.filter(e=>e.read).length');
  await evaluate('document.querySelector(".reader-jump").click()'); await delay(100);
  assert.equal(await evaluate('hist.entries.filter(e=>e.read).length'), beforeRead, 'jump never marks read'); checks.push('jump never marks read');
  await click('С модерацией'); await delay(350);
  await check('moderation left, read right', `(()=>{const r=document.querySelector('.reader-row');const m=r.querySelector('.reader-mod-buttons').getBoundingClientRect(),b=r.querySelector('.reader-message-body').getBoundingClientRect(),c=r.querySelector('.reader-read').getBoundingClientRect();return m.right<=b.left&&c.left>=b.right})()`);
  await screenshot('02-moderation');
  await check('moderation glyphs are centered in both axes', `(()=>{return [...document.querySelectorAll('.reader-row:first-child .reader-mod-buttons button')].every(b=>{const a=b.getBoundingClientRect(),i=b.querySelector('svg').getBoundingClientRect();return Math.abs(a.x+a.width/2-i.x-i.width/2)<=.5&&Math.abs(a.y+a.height/2-i.y-i.height/2)<=.5})})()`);
  await check('viewer rewards use their pictures instead of moderator actions', `['chat:63','chat:64'].every(id=>{const e=document.querySelector('[data-message-id="'+id+'"]');return !!e.querySelector('.reader-reward-marker img')&&!e.querySelector('.reader-mod-buttons button')})`);

  await check('warning lies between deletion and timeout', `(()=>{const a=[...document.querySelector('.reader-row .reader-mod-buttons').children].map(b=>b.getAttribute('aria-label').split(':')[0]);return a.join(',')==='Удалить сообщение,Предупредить,Выдать мут,Забанить'})()`);
  await evaluate(`document.querySelector('[data-message-id="chat:67"] button[aria-label^="Предупредить"]').click()`);await delay(30);
  await check('warning requires a reason before confirmation', `document.querySelector('.reader-action-buttons button.warning').disabled&&document.querySelector('input[aria-label="Причина модерации"]').required`);
  await evaluate(`{const el=document.querySelector('input[aria-label="Причина модерации"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,'Без спойлеров');el.dispatchEvent(new Event('input',{bubbles:true}))}`);await delay(20);
  await screenshot('05-warning');
  await click('Предупредить Viewer67');await delay(80);
  await check('warning sends exact target and reason', `calls.some(c=>c.cmd==='chat_moderate'&&c.args.action==='warn'&&c.args.id==='chat:67'&&c.args.reason==='Без спойлеров'&&c.args.seconds===null)`);

  await evaluate(`document.querySelector('[data-message-id="chat:67"] button[aria-label^="Выдать мут"]').click()`); await delay(50);
  await evaluate('add()'); await delay(180);
  await check('confirmation freezes exact target', `document.querySelector('.reader-action').textContent.includes('Viewer67')&&!document.querySelector('.reader-action').textContent.includes('Viewer68')`);
  await evaluate('window.rejectAction=true'); await click('Выдать мут Viewer67'); await delay(150);
  await check('failed moderation retains dialog and reports error', `document.querySelector('.reader-action-error').textContent.includes('нет прав')&&document.querySelector('.reader-action').textContent.includes('Viewer67')`);
  await evaluate('window.rejectAction=false'); await click('Выдать мут Viewer67'); await delay(150);
  await check('moderation sends fixed message ID', `calls.filter(c=>c.cmd==='chat_moderate'&&c.args.action==='timeout').every(c=>c.args.id==='chat:67'&&c.args.generation===1&&c.args.seconds===600)`);
  await evaluate(`document.querySelector('button[aria-label="Настройки читалки"]').click()`); await delay(50);
  await screenshot('03-settings');
  await check('unread exclusions start with an empty editor', `document.querySelector('.reader-ignored textarea').value===''`);
  await check('all boolean settings use shared capsules', `document.querySelectorAll('.reader-settings .toggle-switch input[type=checkbox]').length===6&&document.querySelectorAll('.reader-settings input[type=checkbox]').length===6`);
  await check('modal keeps the shared round close button', `(()=>{const s=getComputedStyle(document.querySelector('.modal-close-btn'));return s.borderRadius==='50%'&&s.borderTopWidth==='0px'})()`);

  await evaluate(`{const el=document.querySelector('.reader-ignored textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(el,'StreamElements, NewBot,');el.dispatchEvent(new Event('input',{bubbles:true}));}`); await delay(350);
  await check('ignored-nick editor preserves separator while typing', `document.querySelector('.reader-ignored textarea').value.endsWith(',')`);
  await evaluate('document.querySelector(".modal-close-btn").click()');
  await click('Награды за баллы');await delay(150);
  await check('rewards load live channel artwork and form three columns', `(()=>{const a=[...document.querySelectorAll('.reader-reward-tile')].map(e=>e.getBoundingClientRect());return a.length===7&&Math.abs(a[0].width-a[0].height)<1&&a[0].top===a[1].top&&a[1].top===a[2].top&&a[3].top>a[0].top&&document.querySelectorAll('.reader-reward-art img').length===7})()`);
  await check('only matching rewards show text input', `document.querySelectorAll('.reader-reward-input').length===2&&document.querySelector('[data-reward-id="r0"] .reader-reward-input').placeholder==='Кого поприветствовать?'`);
  await check('reward order follows ascending cost', `(()=>{const costs=[...document.querySelectorAll('.reader-reward-cost')].map(e=>Number(e.textContent.replace(/[^0-9]/g,'')));return costs.join(',')==='50,200,300,500,800,900,1000'})()`);
  await check('unconfigured text rewards remain visible with configure action', `(()=>{const r=document.querySelector('[data-reward-id="rw-text"]');return r.querySelector('input').placeholder==='Задайте вопрос'&&r.querySelector('input').disabled&&r.querySelector('.reader-reward-bottom button').textContent==='Настроить'})()`);
  await check('manual reactions clearly remain tests', `document.querySelector('.reader-rewards-note').textContent.includes('Погашение на Twitch не создаётся')`);
  await screenshot('06-rewards');
  await evaluate(`{const el=document.querySelector('[data-reward-id="r0"] .reader-reward-input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,'@SomeViewer');el.dispatchEvent(new Event('input',{bubbles:true}))}`);await delay(20);
  await evaluate(`document.querySelector('[data-reward-id="r0"] .reader-reward-bottom button').click()`);await delay(80);
  await check('reward test receives its own text and streamer identity', `calls.some(c=>c.cmd==='response_test'&&c.args.vars.message==='@SomeViewer'&&c.args.vars.user==='host')`);
  await evaluate(`document.querySelector('.modal-close-btn').click()`);
  await click('Прочитано всё'); await delay(180);
  await evaluate('add()'); await delay(200);
  await check('mark all does not consume future arrivals', `hist.entries[hist.entries.length-1].read===false&&document.querySelector('.reader-toolbar strong').textContent==='1'`);
  await click('Twitch Popout'); await delay(350);
  await check('native mode hides custom controls and requests physical bounds', `document.querySelector('.reader-custom').hidden&&calls.some(c=>c.cmd==='twitch_view'&&c.args.bounds?.width>100)&&!document.querySelector('button[aria-label="Настройки читалки"]')`);
  await check('switching to native does not mark read', `hist.entries[hist.entries.length-1].read===false`);
  await evaluate(`const d=document.createElement('div');d.className='confirm-overlay';d.id='test-dialog';document.body.append(d)`); await delay(80);
  await check('native hides for app confirmation', `calls.filter(c=>c.cmd==='twitch_view').at(-1).args.bounds===null`);
  await evaluate(`document.getElementById('test-dialog').remove()`); await delay(80);
  await check('native returns after app confirmation', `calls.filter(c=>c.cmd==='twitch_view').at(-1).args.bounds!==null`);

  await evaluate(`Array.from(document.querySelectorAll('.sidebar-btn')).find(b=>b.textContent.includes('Заметки')).click()`); await delay(150);
  await check('leaving section hides native view', `calls.filter(c=>c.cmd==='twitch_view').at(-1).args.bounds===null`);
  await evaluate(`Array.from(document.querySelectorAll('.sidebar-btn')).find(b=>b.textContent.includes('Читать чат')).click()`); await delay(100);
  await click('С модерацией'); await delay(350);
  await call('Emulation.setDeviceMetricsOverride', { width: 900, height: 650, deviceScaleFactor: 1, mobile: false }); await delay(120);
  await check('narrow moderation keeps four horizontal buttons', `(()=>{const b=[...document.querySelectorAll('.reader-row:last-child .reader-mod-buttons button')].map(e=>e.getBoundingClientRect());return b.length===4&&b.every(r=>r.y===b[0].y)&&b[0].width===28})()`);
  await screenshot('04-narrow');
  await check('modes and autoscroll remain one row at minimum window width', `(()=>{const m=document.querySelector('.reader-modes').getBoundingClientRect(),t=document.querySelector('.reader-modebar .toggle-switch').getBoundingClientRect();return Math.abs(m.top+m.height/2-t.top-t.height/2)<=1})()`);

  await evaluate(`hist.entries=Array.from({length:500},(_,i)=>row('ring'+i));hist.revision++;emit()`); await delay(200);
  await evaluate(`document.querySelector('.reader-jump')?.click()`); await delay(80);
  const narrowPoint=await evaluate(`(()=>{const r=document.querySelector('.reader-feed').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`);
  await call('Input.dispatchMouseEvent',{type:'mouseWheel',...narrowPoint,deltaY:-500,deltaX:0}); await delay(180);
  const viewport=()=>evaluate(`(()=>{const f=document.querySelector('.reader-feed').getBoundingClientRect();const r=[...document.querySelectorAll('.reader-row')].find(e=>e.getBoundingClientRect().bottom>f.top);return {id:r.dataset.messageId,offset:r.getBoundingClientRect().top-f.top}})()`);
  const anchored=await viewport();
  await evaluate(`hist.entries.shift();add()`); await delay(200);
  {const v=await viewport();assert.equal(v.id,anchored.id);assert.ok(Math.abs(v.offset-anchored.offset)<=1,'ring eviction moves less than one CSS pixel');}checks.push('ring eviction preserves the anchored message');
  await click('Twitch Popout');await delay(300);await click('С модерацией');await delay(300);
  {const v=await viewport();assert.equal(v.id,anchored.id);assert.ok(Math.abs(v.offset-anchored.offset)<=1,'native round trip moves less than one CSS pixel');}checks.push('native round trip preserves paused position');

  await writeFile(out + '/results.json', JSON.stringify({ passed: true, checks }, null, 2)); console.log(JSON.stringify({ passed: true, checks }));
} finally { ws?.close(); chrome.kill('SIGTERM'); server.close(); }
