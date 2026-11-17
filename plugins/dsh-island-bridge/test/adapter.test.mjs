import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDshAdapter } from '../adapter.mjs';

test('DSH 适配器按需读取最近一轮，排除推理和注入内容，不控制 Agent',async()=>{
  let reads=0,disposed=0;const listeners=new Map();
  const header={id:'main',cwd:'F:\\demo',createdAt:1700000000000,origin:'user'};
  const events=[{type:'turn/start',time:1700000000000,data:{turn:3}},{type:'user/message',time:1700000000001,data:{source:{kind:'user'},content:[{type:'text',text:'真正的请求'}]}},{type:'user/message',time:1700000000002,data:{source:{kind:'system'},content:[{type:'text',text:'注入信息'}]}},{type:'todo/write',time:1700000000003,data:{todos:[{status:'completed',content:'检查'},{status:'in_progress',content:'修复'}]}},{type:'request/context',time:1700000000004,data:{contextWindow:10000}},{type:'assistant/message',time:1700000000005,data:{stream:[{type:'text-chunks',texts:['公开','答复']},{type:'chunk',chunk:{type:'reasoning-delta',text:'内部推理'}},{type:'chunk',chunk:{type:'usage',usage:{inputTokens:2000,outputTokens:500}}}]}},{type:'turn/end',time:1700000000006,data:{reason:'completed'}}];
  const session={id:'main',header};const agent={id:'main',session,status:'idle',model:{current:()=>({model:'deepseek-test'})}};
  events.splice(5,0,{type:'tool/call',time:1700000000004,data:{name:'bash',arguments:'不能显示的原始命令'}});
  const ctx={sessions:{list:()=>[session],get:id=>id==='main'?session:undefined},agents:{list:()=>[agent],get:id=>id==='main'?agent:undefined},sessionQuery:{listSessions:async()=>[{header,live:true,persisted:true}],observeSession:async()=>{reads++;return {header,events,[Symbol.dispose]:()=>disposed++};}},on:(name,fn)=>{listeners.set(name,fn);return ()=>listeners.delete(name);}};
  const adapter=createDshAdapter(ctx);const unsubscribe=adapter.subscribe();const summary=await adapter.list();assert.equal(summary.length,1);assert.equal(reads,0,'胶囊不能读取正文');
  const out=await adapter.detail('main');assert.equal(reads,1);assert.equal(disposed,1);assert.deepEqual(out.messages.map(m=>m.text),['真正的请求','公开答复']);assert.deepEqual(out.tools,['bash'],'工具摘要与正文分开');assert.equal(out.state,'completed');assert.deepEqual(out.plan,{completed:1,total:2,current:'修复'});assert.deepEqual(out.context,{used:2500,total:10000});
  listeners.get('session/event')(session,{type:'session/title',time:1700000001000,data:{title:'更新标题'}});assert.equal((await adapter.list())[0].updatedAt,out.updatedAt,'标题更新不能抢占活跃会话');
  listeners.get('session/event')(session,{type:'approval/asked',time:1700000000010,data:{toolName:'bash'}});assert.equal((await adapter.list())[0].state,'waiting');
  listeners.get('session/event')(session,{type:'approval/decided',time:1700000000011,data:{outcome:'allowed-once'}});assert.equal((await adapter.list())[0].state,'working');
  unsubscribe();assert.equal(listeners.size,0);
});

test('达到 token 上限、取消和未知结束原因不能显示为成功',async()=>{
  for(const [reason,expected] of [['max-tokens','interrupted'],['aborted','interrupted'],['interrupted','interrupted'],['unknown','interrupted'],['error','failed'],['completed','completed']]){
    const header={id:'main',cwd:'F:\\demo',createdAt:1000};
    const ctx={sessions:{get:()=>({header})},sessionQuery:{observeSession:async()=>({header,events:[{type:'turn/start',time:1000,data:{turn:1}},{type:'turn/end',time:2000,data:{reason:{kind:reason}}}],[Symbol.dispose]:()=>{}})}};
    assert.equal((await createDshAdapter(ctx).detail('main')).state,expected);
  }
});
