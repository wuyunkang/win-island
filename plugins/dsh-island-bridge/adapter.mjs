// DSH 0.2.2 的公开只读 Session/Agent API；不注册工具或审批 answerer。
import { normalizeSnapshot,validId } from './bridge.mjs';

const stamp=value=>typeof value==='number'&&Number.isFinite(value)?Math.trunc(value):typeof value==='string'?Date.parse(value)||0:0;
function textOf(data,limit=64*1024,onTruncate=()=>{}){
  const parts=[];let bytes=0;
  const add=text=>{if(typeof text!=='string')return;if(bytes>=limit){if(text.length)onTruncate();return;}const room=limit-bytes;let buf=Buffer.from(text);if(buf.length>room){onTruncate();let end=room;while(end>0&&(buf[end]&0xc0)===0x80)end--;buf=buf.subarray(0,end);}bytes+=buf.length;parts.push(buf.toString('utf8'));};
  if(Array.isArray(data.stream)){for(const record of data.stream){if(record.type==='text-chunks')for(const text of record.texts??[])add(text);else if(record.type==='chunk'&&record.chunk?.type==='text-delta')add(record.chunk.text);}}
  else if(Array.isArray(data.content)){for(const block of data.content)if(block.type==='text'||block.type==='input_text'||block.type==='output_text')add(block.text);}
  else if(typeof data.content==='string')add(data.content);
  return parts.join('');
}
function applyEvent(row,event,body=false){
  const data=event.data??{},at=stamp(event.time??event.timestamp);let meaningful=true;
  switch(event.type){
    case 'session/title':row.title=String(data.title??'').slice(0,160);meaningful=false;break;
    case 'turn/start':row.turnId=String(data.turn??event.seq??'');row.state='thinking';row.startedAt=at;delete row.endedAt;delete row.plan;delete row.context;delete row._contextUsed;row.tools=[];if(body){row.messages=[];row.truncated=false;}break;
    case 'turn/end':{
      const reason=typeof data.reason==='string'?data.reason:data.reason?.kind;
      row.state=['error','failed','failure'].includes(reason)?'failed':reason==='completed'?'completed':'interrupted';row.endedAt=at;break;
    }
    case 'user/message':{
      if(!(data.source?.kind==='user'||data.source==='user')){meaningful=false;break;}
      const text=textOf(data,body?64*1024:240,()=>{if(body)row.truncated=true;});if(!text.trim()){meaningful=false;break;}
      if(!row.title)row.title=text.split('\n')[0].slice(0,80);if(body)row.messages.push({kind:'user',text});break;
    }
    case 'assistant/message':{
      if(body){const text=textOf(data,64*1024,()=>row.truncated=true);if(text.trim())row.messages.push({kind:'answer',text});}
      const usage=data.stream?.findLast?.(r=>r.type==='chunk'&&r.chunk?.type==='usage')?.chunk?.usage;
      const input=usage?.inputTokens??usage?.input?.total??usage?.input,output=usage?.outputTokens??usage?.output?.total??usage?.output;
      if(Number.isSafeInteger(input)&&input>=0&&Number.isSafeInteger(output)&&output>=0)row._contextUsed=input+output;else delete row._contextUsed;
      if(row._contextTotal>0&&row._contextUsed>=0)row.context={used:row._contextUsed,total:row._contextTotal};
      if(!row.endedAt)row.state='answering';break;
    }
    case 'request/context':row.model=String(data.model??row.model??'');row._contextTotal=data.contextWindow??data.context?.contextWindow??data.capacity?.contextWindow;if(!row._contextTotal)delete row.context;meaningful=false;break;
    case 'todo/write':{
      const todos=Array.isArray(data.todos)?data.todos:[];row.plan={completed:todos.filter(t=>['completed','done'].includes(t.status)).length,total:todos.length,current:String(todos.find(t=>['in_progress','doing'].includes(t.status))?.content??'')};meaningful=false;break;
    }
    case 'approval/asked':case 'question/asked':case 'user-question/asked':row.state='waiting';break;
    case 'approval/decided':case 'question/answered':case 'user-question/answered':row.state='working';break;
    case 'tool/call':case 'tool/request':{
      const name=data.name??data.toolName??data.call?.name;if(typeof name==='string'){row.tools=[...(row.tools??[]),name].slice(-3);}if(!row.endedAt)row.state='working';break;
    }
    default:meaningful=false;
  }
  if(meaningful&&at>0)row.updatedAt=Math.max(row.updatedAt??0,at);
}

export function createDshAdapter(ctx){
  const cache=new Map(),viewed=new Map();let inventory=[],inventoryAt=0,inventoryJob;
  const service=name=>{try{return ctx[name]??ctx.get?.(name);}catch{return undefined;}};
  const remember=(header,agent)=>{
    if(!header||!validId(header.id)||header.origin==='subagent')return null;
    let row=cache.get(header.id);
    if(!row){row={id:header.id,cwd:header.cwd??'',title:'',model:'',state:agent?.status==='running'?'working':'unknown',updatedAt:agent?.status==='running'?Date.now():stamp(header.createdAt),tools:[],messages:[]};cache.set(row.id,row);}
    try{const selection=agent?.model?.current?.();if(selection?.model)row.model=String(selection.model);}catch{}
    if(cache.size>64){const oldest=[...cache.values()].sort((a,b)=>a.updatedAt-b.updatedAt)[0];if(oldest.id!==row.id)cache.delete(oldest.id);}
    return row;
  };
  const summary=async id=>{
    const agent=service('agents')?.get?.(id),session=service('sessions')?.get?.(id);const header=session?.header??inventory.find(s=>s.header?.id===id)?.header;
    const row=remember(header,agent);return row?normalizeSnapshot(row):null;
  };
  return {
    markRead(ids,at=Date.now()){
      if(!Number.isSafeInteger(at)||at<=0)return;
      for(const id of ids.slice(0,64)){if(!validId(id))continue;viewed.delete(id);viewed.set(id,at);}
      while(viewed.size>128)viewed.delete(viewed.keys().next().value);
    },
    readReceipts(){return [...viewed].map(([id,viewedAt])=>({id,viewedAt}));},
    async list(signal){
      if(!inventoryAt||Date.now()-inventoryAt>10000){inventoryJob??=(async()=>{inventory=await service('sessionQuery')?.listSessions(signal)??[];inventoryAt=Date.now();})().finally(()=>inventoryJob=undefined);await inventoryJob;}
      const agents=Array.from(service('agents')?.list?.()??[]);const headers=inventory.slice(0,32).map(s=>s.header);
      for(const agent of agents)if(!headers.some(h=>h.id===agent.id))headers.push(agent.session.header);
      for(const header of headers)remember(header,agents.find(a=>a.id===header.id));
      return [...cache.values()].map(row=>normalizeSnapshot(row)).filter(Boolean).sort((a,b)=>b.updatedAt-a.updatedAt).slice(0,32);
    },summary,
    async detail(id,signal){
      if(!validId(id))throw new Error('会话标识无效');
      const query=service('sessionQuery');if(!query?.observeSession)throw new Error('DSH 版本缺少只读会话查询接口');
      const observation=await query.observeSession(id,{projectionMode:'none',signal});
      try{
        if(observation.header.origin==='subagent')return null;
        const row={...(await summary(id)??{id,cwd:observation.header.cwd??'',title:'',updatedAt:stamp(observation.header.createdAt)}),messages:[],tools:[]};
        // 只折叠尾部事件；不保留观察租约或完整历史，不调用已弃用的同步事件接口。
        const events=observation.events;row.truncated=events.length>2048;
        for(const event of events.slice(-2048)){applyEvent(row,event,true);if(row.messages.length>64){row.messages.splice(1,1);row.truncated=true;}}
        if(row._contextTotal>0&&row._contextUsed>=0)row.context={used:row._contextUsed,total:row._contextTotal};
        const normalized=normalizeSnapshot(row,true);if(normalized)cache.set(id,{...normalizeSnapshot(normalized),_contextTotal:row._contextTotal,_contextUsed:row._contextUsed});return normalized;
      }finally{observation[Symbol.dispose]?.();}
    },
    subscribe(){
      const disposers=[];const on=(name,callback)=>{const dispose=ctx.on(name,(...args)=>{try{callback(...args);}catch{}},{global:true});if(typeof dispose==='function')disposers.push(dispose);};
      on('session/event',(session,event)=>{const row=remember(session.header,service('agents')?.get?.(session.id));if(row)applyEvent(row,event);});
      on('agent/status',({agent,status})=>{const row=remember(agent.session.header,agent);if(row&&status==='running'){row.state='thinking';row.updatedAt=Date.now();delete row.endedAt;}});
      on('agent/assistant-stream',({agent,frame})=>{
        const row=remember(agent.session.header,agent);if(!row||row.endedAt)return;
        if(frame.type==='chunk'){
          if(frame.chunk?.type==='reasoning-delta')row.state='thinking';
          if(frame.chunk?.type==='text-delta')row.state='answering';
          if(frame.chunk?.type==='tool-call-delta'&&frame.chunk.name){row.state='working';row.tools=[...(row.tools??[]),String(frame.chunk.name)].slice(-3);}
          row.updatedAt=stamp(frame.time)||row.updatedAt;
        }
      });
      return ()=>{for(const dispose of disposers)dispose();cache.clear();inventory=[];inventoryAt=0;};
    },
  };
}
