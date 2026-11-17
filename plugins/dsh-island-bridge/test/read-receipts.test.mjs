import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile,mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import vm from 'node:vm';
import {createDshAdapter} from '../adapter.mjs';
import {startBridge} from '../bridge.mjs';
import {registerReadReports} from '../index.mjs';

test('已读上报复用 DSH 认证连接，并拒绝无效会话及客户端时间',async()=>{
  let handler,disposed=false;const received=[];
  const ctx={effect(fn){this.dispose=fn()},connection:{fetch:{register(route){assert.equal(route.path,'/api/win-island/viewed');handler=async(method,payload)=>{
    const response=await route.fetch(new Request('http://localhost/api/win-island/viewed',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({type:'client-request',rpcId:'test-read',method,payload})}));
    const out=await response.json();assert.equal(out.rpcId,'test-read');return out.result;
  };return ()=>{disposed=true}}}}};
  registerReadReports(ctx,{markRead(ids,...extra){received.push({ids,extra})}});
  assert.equal((await handler('win-island/viewed',{readSessionIds:['one'],viewedAt:Number.MAX_SAFE_INTEGER})).ok,true);
  assert.deepEqual(received,[{ids:['one'],extra:[]}],'宿主自行取时间，不能接受客户端伪造的未来回执');
  for(const payload of [null,{}, {readSessionIds:['bad/id']},{readSessionIds:Array(65).fill('one')}])assert.equal((await handler('win-island/viewed',payload)).ok,false);
  assert.equal((await handler('other',{readSessionIds:['one']})).ok,false);assert.equal(received.length,1);
  ctx.dispose();assert.equal(disposed,true);
});

test('DSH 已读回执只记录明确查看的会话，不修改任务或保存正文',()=>{
  const adapter=createDshAdapter({});
  assert.equal(typeof adapter.markRead,'function');
  adapter.markRead(['one','two'],1000);
  adapter.markRead(['one'],2000);
  adapter.markRead(['bad/id'],3000);
  assert.deepEqual(adapter.readReceipts().sort((a,b)=>a.id.localeCompare(b.id)),[{id:'one',viewedAt:2000},{id:'two',viewedAt:1000}]);
  assert.equal(JSON.stringify(adapter.readReceipts()).includes('messages'),false);
});

test('DSH 快照独立传递已读回执，最近列表不含该会话时仍可清除',async()=>{
  const root=await mkdtemp(join(tmpdir(),'island-dsh-read-'));
  const bridge=await startBridge({discoveryPath:join(root,'bridge.json'),adapter:{list:async()=>[],readReceipts:()=>[{id:'viewed-old',viewedAt:1000}],subscribe:()=>()=>{}}});
  try{
    const d=JSON.parse(await readFile(join(root,'bridge.json'),'utf8'));
    const out=await(await fetch(`${d.endpoint}/snapshot`,{method:'POST',headers:{Authorization:`Bearer ${d.token}`,'Content-Type':'application/json'},body:JSON.stringify({full:false})})).json();
    assert.deepEqual(out.readReceipts,[{id:'viewed-old',viewedAt:1000}]);
    assert.equal(out.sessions.length,0);
  }finally{await bridge.stop();await rm(root,{recursive:true,force:true});}
});

test('DSH 客户端只确认主视图会话；未读、其他会话和未就绪列表保持原状',async()=>{
  let entry;let heartbeat;let timerStopped=false;let focused=true;const disposers=[];const calls=[];const events=new Map();
  const source=value=>({value,listeners:new Set(),getSnapshot(){return this.value},subscribe(fn){this.listeners.add(fn);return ()=>this.listeners.delete(fn)},emit(){for(const fn of this.listeners)fn()}});
  const list=source({phase:'pending',byId:{}}),status=source(new Map());
  const ctx={sessions:{list},uiSession:{sessionStatus:status},connection:{rpc:{call:async(channel,method,payload)=>{calls.push({channel,method,payload:JSON.parse(JSON.stringify(payload))});return {ok:true,value:{accepted:true}}}}},effect(fn){disposers.push(fn())}};
  const dom={addEventListener(name,fn){events.set(name,fn)},removeEventListener(name){events.delete(name)}};
  const document={...dom,visibilityState:'visible',hasFocus:()=>focused};
  vm.runInNewContext(await readFile(new URL('../client.js',import.meta.url),'utf8'),{window:{...dom,__ModuleLoader__:{load(value){entry=value}}},document,setInterval(fn){heartbeat=fn;return 1},clearInterval(){timerStopped=true},AbortController});
  const plugin=entry.factory(()=>{});plugin.apply(ctx);await new Promise(setImmediate);
  assert.equal(calls.length,0,'列表未就绪不推断已读');
  list.value={phase:'ready',byId:{one:{id:'one',retainedBy:{mainView:1}},two:{id:'two',retainedBy:{mainView:0}}}};
  status.value=new Map([['one',{completionUnread:false}],['two',{completionUnread:true}]]);list.emit();await new Promise(setImmediate);
  assert.deepEqual(calls.at(-1).payload.readSessionIds,['one']);
  list.value.byId.one.retainedBy.mainView=0;list.value.byId.two.retainedBy.mainView=1;status.emit();await new Promise(setImmediate);
  assert.deepEqual(calls.at(-1).payload.readSessionIds,['two']);
  heartbeat();await new Promise(setImmediate);assert.deepEqual(calls.at(-1).payload.readSessionIds,['two']);
  const before=calls.length;focused=false;heartbeat();await new Promise(setImmediate);assert.equal(calls.length,before,'后台窗口不能自动确认新提醒');
  focused=true;events.get('focus')();await new Promise(setImmediate);assert.equal(calls.length,before+1,'从其他应用切回时不必点击通知也要同步已读');
  document.visibilityState='hidden';heartbeat();await new Promise(setImmediate);assert.equal(calls.length,before+1);
  for(const dispose of disposers)dispose?.();assert.equal(timerStopped,true);assert.equal(list.listeners.size,0);assert.equal(status.listeners.size,0);assert.equal(events.size,0);
});
