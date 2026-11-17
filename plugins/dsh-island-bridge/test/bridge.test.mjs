import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startBridge } from '../bridge.mjs';

test('桥接只在展开时获取正文，认证失败和网页请求不能读取会话', async () => {
  const root=await mkdtemp(join(tmpdir(),'island-dsh-'));
  let details=0,attached=0;
  const adapter={list:async()=>[{id:'one',cwd:'F:\\demo',title:'公开标题',state:'working',updatedAt:1700000000000,secret:'不能导出'}],detail:async()=>{details++;return {id:'one',cwd:'F:\\demo',title:'公开标题',state:'working',updatedAt:1700000000000,messages:[{kind:'user',text:'公开请求'},{kind:'reasoning',text:'不能显示'},{kind:'answer',text:'公开答复'}]};},subscribe:()=>{attached++;return ()=>attached--;}};
  const path=join(root,'bridge.json');const bridge=await startBridge({discoveryPath:path,adapter,idleMs:60});
  try {
    const discovery=JSON.parse(await readFile(path,'utf8'));assert.equal(discovery.version,1);assert.match(discovery.endpoint,/^http:\/\/127\.0\.0\.1:\d+$/);
    const request=(body,extra={})=>fetch(`${discovery.endpoint}/snapshot`,{method:'POST',headers:{Authorization:`Bearer ${discovery.token}`,'Content-Type':'application/json',...extra},body:JSON.stringify(body)});
    assert.equal((await request({full:false},{Authorization:'Bearer wrong'})).status,401);
    assert.equal((await request({full:false},{Origin:'https://example.com'})).status,403);
    const summary=await(await request({full:false})).json();assert.equal(details,0);assert.equal(summary.sessions.length,1);assert.equal(summary.selected.messages.length,0);assert.equal(JSON.stringify(summary).includes('不能导出'),false);
    const full=await(await request({full:true,selected:'one'})).json();assert.equal(details,1);assert.deepEqual(full.selected.messages.map(m=>m.text),['公开请求','公开答复']);
    assert.equal(attached,1);await new Promise(r=>setTimeout(r,120));assert.equal(attached,0);
  } finally {await bridge.stop();await rm(root,{recursive:true,force:true});}
});

test('正文和候选有硬上限，停止后删除发现文件并释放订阅', async()=>{
  const root=await mkdtemp(join(tmpdir(),'island-dsh-cap-'));let released=0;
  const sessions=Array.from({length:50},(_,i)=>({id:`s-${i}`,state:'completed',updatedAt:1700000000000+i,title:'任务'}));
  const bridge=await startBridge({discoveryPath:join(root,'bridge.json'),adapter:{list:async()=>sessions,detail:async(id)=>({...sessions.find(s=>s.id===id),messages:Array.from({length:100},()=>({kind:'answer',text:'答复'.repeat(20000)}))}),subscribe:()=>()=>released++}});
  try {const d=JSON.parse(await readFile(join(root,'bridge.json'),'utf8'));const out=await(await fetch(`${d.endpoint}/snapshot`,{method:'POST',headers:{Authorization:`Bearer ${d.token}`,'Content-Type':'application/json'},body:JSON.stringify({full:true})})).json();assert.equal(out.sessions.length,32);assert.ok(out.selected.messages.length<=64);assert.ok(out.selected.messages.reduce((n,m)=>n+Buffer.byteLength(m.text),0)<=256*1024);assert.equal(out.selected.truncated,true);}finally{await bridge.stop();assert.equal(released,1);await assert.rejects(readFile(join(root,'bridge.json')));await rm(root,{recursive:true,force:true});}
});
