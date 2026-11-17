import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,readFile,rm,copyFile} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {install,removeManagedBlock} from '../install.mjs';

test('安装和更新保留其他配置，卸载只删除自有区块',async()=>{
  const root=await mkdtemp(join(tmpdir(),'island-dsh-install-'));const home=join(root,'dsh'),dataDir=join(root,'island');const path=join(home,'profiles','desktop','cordis.patch.yml');const original='# 用户配置\r\n- id: existing\r\n  config:\r\n    token: demo-only\r\n';await mkdir(join(home,'profiles','desktop'),{recursive:true});await writeFile(path,original);
  try{const first=await install({home,dataDir});assert.equal(first.installed,true);const text=await readFile(path,'utf8');assert.equal(removeManagedBlock(text),original);await install({home,dataDir});assert.equal((await readFile(path,'utf8')).split('id: win-island-dsh-bridge').length,2);await install({home,dataDir,uninstall:true});assert.equal(await readFile(path,'utf8'),original);assert.equal(await readFile(first.backup,'utf8'),original);}finally{await rm(root,{recursive:true,force:true});}
});

test('代码更新更换 Cordis 条目，避免同一条目继续运行旧模块',async()=>{
  const root=await mkdtemp(join(tmpdir(),'island-dsh-update-'));const home=join(root,'dsh'),dataDir=join(root,'island'),source=join(root,'source');const patch=join(home,'profiles','desktop','cordis.patch.yml');
  await mkdir(join(home,'profiles','desktop'),{recursive:true});await mkdir(source);await writeFile(patch,'[]\n');
  for(const name of ['index.mjs','adapter.mjs','bridge.mjs','package.json'])await copyFile(new URL(`../${name}`,import.meta.url),join(source,name));
  try{
    const first=await install({home,dataDir,source});const before=await readFile(patch,'utf8');
    assert.match(before,new RegExp(`id: win-island-dsh-bridge-${first.version}\\b`));
    await writeFile(join(source,'index.mjs'),(await readFile(join(source,'index.mjs'),'utf8'))+'\n// 新版本\n');
    const second=await install({home,dataDir,source});const after=await readFile(patch,'utf8');
    assert.notEqual(second.version,first.version);assert.match(after,new RegExp(`id: win-island-dsh-bridge-${second.version}\\b`));assert.ok(!after.includes(`id: win-island-dsh-bridge-${first.version}`));
    assert.match(await readFile(join(dataDir,'plugins','dsh-bridge',first.version,'index.mjs'),'utf8'),/export async function apply/);
  }finally{await rm(root,{recursive:true,force:true});}
});
