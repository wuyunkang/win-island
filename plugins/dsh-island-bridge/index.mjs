import { homedir } from 'node:os';
import { join } from 'node:path';
import { startBridge } from './bridge.mjs';
import { createDshAdapter } from './adapter.mjs';

export const name='win-island-dsh-bridge';
export const inject=['agents','sessions','sessionQuery'];
export async function apply(ctx,config={}){
  const directory=process.env.WIN_ISLAND_DATA_DIR??join(process.env.APPDATA??join(homedir(),'AppData','Roaming'),'WinIslandRust');
  const bridge=await startBridge({discoveryPath:config.discoveryPath??join(directory,'dsh-bridge.json'),adapter:createDshAdapter(ctx)});
  ctx.on('dispose',()=>bridge.stop());
}
