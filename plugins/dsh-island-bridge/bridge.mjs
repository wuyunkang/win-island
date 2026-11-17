// Copyright (c) 灵动岛 contributors. MIT.
import { createServer } from 'node:http';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { mkdir, readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';

const MAX_TEXT=256*1024,MAX_MESSAGES=64,MAX_REQUEST=4096;
const states=new Set(['unknown','idle','thinking','working','answering','waiting','completed','interrupted','failed']);
export function validId(id){return typeof id==='string'&&/^[A-Za-z0-9_.-]{1,128}$/.test(id);}
function boundedText(value,bytes){
  if(typeof value!=='string')return '';
  const b=Buffer.from(value);if(b.length<=bytes)return value;
  let end=bytes;while(end>0&&(b[end]&0xc0)===0x80)end--;return b.subarray(0,end).toString('utf8');
}
export function normalizeSnapshot(raw,full=false){
  if(!raw||!validId(raw.id)||raw.subagent)return null;
  const time=value=>Number.isSafeInteger(value)&&value>=0?value:undefined;
  const result={id:raw.id,cwd:boundedText(raw.cwd,2048),title:boundedText(raw.title,240),model:boundedText(raw.model,100),state:states.has(raw.state)?raw.state:'unknown',updatedAt:time(raw.updatedAt)??0,turnId:boundedText(String(raw.turnId??''),128),messages:[],truncated:raw.truncated===true};
  for(const name of ['startedAt','endedAt']){const value=time(raw[name]);if(value!==undefined)result[name]=value;}
  if(raw.context&&Number.isSafeInteger(raw.context.used)&&raw.context.used>=0&&Number.isSafeInteger(raw.context.total)&&raw.context.total>0)result.context={used:raw.context.used,total:raw.context.total};
  if(raw.plan&&Number.isSafeInteger(raw.plan.total)&&raw.plan.total>0&&raw.plan.total<=1000&&Number.isSafeInteger(raw.plan.completed)&&raw.plan.completed>=0&&raw.plan.completed<=raw.plan.total)result.plan={completed:raw.plan.completed,total:raw.plan.total,current:boundedText(raw.plan.current,160)};
  result.tools=Array.isArray(raw.tools)?raw.tools.filter(t=>typeof t==='string').slice(-3).map(t=>boundedText(t,80)):[];
  if(full&&Array.isArray(raw.messages)){
    let used=0;
    for(const message of raw.messages){
      if(!['user','progress','answer'].includes(message?.kind)||typeof message.text!=='string'||!message.text.trim())continue;
      if(result.messages.length>=MAX_MESSAGES||used>=MAX_TEXT){result.truncated=true;break;}
      const text=boundedText(message.text,Math.min(64*1024,MAX_TEXT-used));used+=Buffer.byteLength(text);result.messages.push({kind:message.kind,text});result.truncated ||=text.length<message.text.length;
    }
  }
  return result;
}

export async function startBridge({discoveryPath,adapter,idleMs=5000}){
  const token=randomBytes(32).toString('hex');let detach,timer,stopping=false;
  const release=()=>{clearTimeout(timer);timer=undefined;if(detach){const stop=detach;detach=undefined;try{stop();}catch{}}};
  const touch=()=>{if(!detach)detach=adapter.subscribe?.()??(()=>{});clearTimeout(timer);timer=setTimeout(release,idleMs);timer.unref();};
  const server=createServer(async(req,res)=>{
    const controller=new AbortController();res.once('close',()=>{if(!res.writableEnded)controller.abort();});
    const reply=(code,body)=>{if(res.destroyed)return;const text=JSON.stringify(body);res.writeHead(code,{'Content-Type':'application/json; charset=utf-8','Cache-Control':'no-store','Connection':'close','X-Content-Type-Options':'nosniff'});res.end(text);};
    try{
      if(stopping)return reply(503,{error:'桥接正在关闭'});
      if(req.headers.origin!==undefined)return reply(403,{error:'不接受网页来源'});
      const supplied=Buffer.from(req.headers.authorization??''),expected=Buffer.from(`Bearer ${token}`);
      if(supplied.length!==expected.length||!timingSafeEqual(supplied,expected))return reply(401,{error:'认证失败'});
      if(req.method!=='POST'||req.url!=='/snapshot')return reply(404,{error:'只提供只读会话快照'});
      if(!String(req.headers['content-type']??'').startsWith('application/json'))return reply(415,{error:'需要 JSON'});
      if(Number(req.headers['content-length'])>MAX_REQUEST)return reply(413,{error:'请求过大'});
      let bytes=0;const chunks=[];
      for await(const chunk of req){bytes+=chunk.length;if(bytes>MAX_REQUEST)return reply(413,{error:'请求过大'});chunks.push(chunk);}
      let body;try{body=JSON.parse(Buffer.concat(chunks).toString('utf8'));}catch{return reply(400,{error:'JSON 无效'});}
      if(!body||typeof body!=='object'||Array.isArray(body)||typeof body.full!=='boolean'||(body.selected!==undefined&&body.selected!==null&&!validId(body.selected)))return reply(400,{error:'请求字段无效'});
      touch();
      const rows=await adapter.list(controller.signal);
      const sessions=Array.from(rows??[]).map(s=>normalizeSnapshot(s)).filter(Boolean).sort((a,b)=>b.updatedAt-a.updatedAt).slice(0,32);
      const selectedId=body.selected??sessions[0]?.id;
      let selected=sessions.find(s=>s.id===selectedId)??null;
      if(body.full&&selectedId){selected=normalizeSnapshot(await adapter.detail(selectedId,controller.signal),true);}
      else if(!selected&&selectedId&&adapter.summary){selected=normalizeSnapshot(await adapter.summary(selectedId,controller.signal),false);}
      const readReceipts=Array.from(adapter.readReceipts?.()??[]).slice(0,128)
        .filter(r=>validId(r?.id)&&Number.isSafeInteger(r.viewedAt)&&r.viewedAt>0)
        .map(r=>({id:r.id,viewedAt:r.viewedAt}));
      reply(200,{version:1,sessions,selected,readReceipts});
    }catch{reply(503,{error:'DSH 会话暂不可读'});}
  });
  server.requestTimeout=2000;server.headersTimeout=2000;server.maxConnections=4;
  await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
  const endpoint=`http://127.0.0.1:${server.address().port}`;
  try{await mkdir(dirname(discoveryPath),{recursive:true});const temporary=`${discoveryPath}.${process.pid}.tmp`;await writeFile(temporary,JSON.stringify({version:1,endpoint,token,pid:process.pid}),{mode:0o600});await rename(temporary,discoveryPath);}catch(error){server.closeAllConnections();await new Promise(r=>server.close(r));throw error;}
  let stopped;
  return {endpoint,stop(){return stopped??=(async()=>{stopping=true;release();server.closeAllConnections();await new Promise(r=>server.close(r));try{const current=JSON.parse(await readFile(discoveryPath,'utf8'));if(current.token===token)await unlink(discoveryPath);}catch{}})();}};
}
