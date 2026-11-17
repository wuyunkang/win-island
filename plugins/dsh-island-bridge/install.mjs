// 仅修改自有的 profile patch 区块；不解析或输出用户配置、登录凭据。
import { mkdir,readFile,writeFile,copyFile,rename,unlink } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname,join,resolve } from 'node:path';
import { fileURLToPath,pathToFileURL } from 'node:url';
import { createHash,randomBytes } from 'node:crypto';

const BEGIN='# BEGIN WIN-ISLAND-DSH-BRIDGE',END='# END WIN-ISLAND-DSH-BRIDGE';
export function removeManagedBlock(text){
  const begin=text.indexOf(BEGIN);if(begin<0)return text;const end=text.indexOf(END,begin);if(end<0)throw new Error('桥接配置区块不完整，请恢复备份后重试');
  return text.slice(0,begin)+text.slice(end+END.length).replace(/^\r?\n/,'');
}
export async function install({home=process.env.DSH_HOME??join(homedir(),'.dsh'),profile='desktop',dataDir=process.env.WIN_ISLAND_DATA_DIR??join(process.env.APPDATA??join(homedir(),'AppData','Roaming'),'WinIslandRust'),source=dirname(fileURLToPath(import.meta.url)),uninstall=false}={}){
  if(!['desktop','web'].includes(profile))throw new Error('仅支持 desktop 或 web profile');
  const profileDir=join(resolve(home),'profiles',profile),patchPath=join(profileDir,'cordis.patch.yml');
  const original=await readFile(patchPath,'utf8');let clean=removeManagedBlock(original);
  if(!uninstall&&/^\s*-?\s*id:\s*['"]?win-island-dsh-bridge(?:-[0-9a-f]{16})?['"]?\s*$/m.test(clean))throw new Error('已有同名插件配置，请先在 DSH 插件管理中停用旧条目');
  const backup=`${patchPath}.win-island-${Date.now()}-${randomBytes(4).toString('hex')}.bak`;await copyFile(patchPath,backup);
  const replace=async text=>{const temporary=`${patchPath}.win-island-${process.pid}.tmp`;try{await writeFile(temporary,text);if(await readFile(patchPath,'utf8')!==original)throw new Error('DSH 配置已被更新，请重试安装');await rename(temporary,patchPath);}catch(error){await unlink(temporary).catch(()=>{});throw error;}};
  if(uninstall){await replace(clean.trim()?clean:'[]\n');return {profile,patchPath,backup,removed:true};}
  const files=['index.mjs','adapter.mjs','bridge.mjs','package.json'];const contents=await Promise.all(files.map(name=>readFile(join(source,name))));
  const hash=createHash('sha256');for(const bytes of contents)hash.update(bytes);const version=hash.digest('hex').slice(0,16);
  const destination=join(resolve(dataDir),'plugins','dsh-bridge',version);await mkdir(destination,{recursive:true});
  for(let i=0;i<files.length;i++)await writeFile(join(destination,files[i]),contents[i]);
  const eol=original.includes('\r\n')?'\r\n':'\n';
  // 空 profile 的 [] 与后续数组项不能并列；其语义保持为空数组。
  clean=clean.replace(/^\s*\[\s*\]\s*$/gm,'');if(clean&&!clean.endsWith('\n'))clean+=eol;
  // Cordis 的同 id 更新不会重新导入 name；更换 id 才会卸载旧模块并导入新版本。
  const lines=[BEGIN,'- insert:',`    - id: win-island-dsh-bridge-${version}`,`      name: ${JSON.stringify(pathToFileURL(join(destination,'index.mjs')).href)}`,'      config:',`        discoveryPath: ${JSON.stringify(join(resolve(dataDir),'dsh-bridge.json'))}`,END,''];
  await replace(clean+lines.join(eol));
  return {profile,patchPath,backup,installed:true,version};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
  const args=process.argv.slice(2);const value=(flag)=>{const i=args.indexOf(flag);return i>=0?args[i+1]:undefined;};
  try{const result=await install({home:value('--home'),profile:value('--profile')??'desktop',dataDir:value('--data-dir'),uninstall:args.includes('--uninstall')});console.log(JSON.stringify(result));}catch(error){console.error(`桥接安装失败：${error.message}`);process.exitCode=1;}
}
