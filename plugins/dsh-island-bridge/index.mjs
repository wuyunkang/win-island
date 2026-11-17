import { homedir } from 'node:os';
import { join } from 'node:path';
import { startBridge } from './bridge.mjs';
import { createDshAdapter } from './adapter.mjs';

export const name='win-island-dsh-bridge';
export const inject=['agents','sessions','sessionQuery','connection'];
export function registerReadReports(ctx,adapter){
  // 共享 /api 的精确路由由 DSH 限定为已认证的 owner Peer，兼容桌面 IPC。
  ctx.effect(()=>ctx.connection.fetch.register({path:'/api/win-island/viewed',methods:['POST'],requestBody:'buffered',fetch:async request=>{
    if(!request.headers.get('content-type')?.startsWith('application/json'))return new Response('需要 JSON',{status:415});
    const text=await request.text();if(text.length>16384)return new Response('回执过大',{status:413});
    let message;try{message=JSON.parse(text);}catch{return new Response('JSON 无效',{status:400});}
    if(message?.type!=='client-request'||typeof message.rpcId!=='string'||message.rpcId.length>128)return new Response('回执信封无效',{status:400});
    const payload=message.payload;
    let result;
    if(message.method!=='win-island/viewed'||!payload||!Array.isArray(payload.readSessionIds)||payload.readSessionIds.length>64
      ||payload.readSessionIds.some(id=>typeof id!=='string'||!/^[A-Za-z0-9_.-]{1,128}$/.test(id))){
      result={ok:false,error:{code:'win-island/invalid-read-receipt',message:'已读回执无效',details:{}}};
    }else{
      // 使用宿主时间；不接收客户端传来的时间、正文或任务状态。
      adapter.markRead(payload.readSessionIds);result={ok:true,value:{accepted:true}};
    }
    return Response.json({type:'server-response',rpcId:message.rpcId,result});
  }}));
}
export async function apply(ctx,config={}){
  const directory=process.env.WIN_ISLAND_DATA_DIR??join(process.env.APPDATA??join(homedir(),'AppData','Roaming'),'WinIslandRust');
  const adapter=createDshAdapter(ctx);
  const bridge=await startBridge({discoveryPath:config.discoveryPath??join(directory,'dsh-bridge.json'),adapter});
  try{registerReadReports(ctx,adapter);ctx.on('dispose',()=>bridge.stop());}
  catch(error){await bridge.stop();throw error;}
}
