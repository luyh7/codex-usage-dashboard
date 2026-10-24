// Exercise the production Rust serializer through GLib's actual desktop-entry
// launcher. A string comparison alone would miss its two unescape layers.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,mkdir,chmod,rm} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';

test('Linux login entry launches literal Unicode, whitespace, shell characters and field codes', {skip:process.platform!=='linux'}, async()=>{
  const lab=await mkdtemp(path.join(os.tmpdir(),'cousash-desktop-entry-'));
  const source=fileURLToPath(new URL('../desktop/src/desktop_entry.rs',import.meta.url));
  try {
    const driver=path.join(lab,'driver.rs'),binary=path.join(lab,'driver');
    await writeFile(driver,`#[path=${JSON.stringify(source)}] mod desktop_entry; fn main(){print!("{}",desktop_entry::command(&std::env::args().nth(1).unwrap()));}`);
    const built=spawnSync('rustc',['--edition=2024',driver,'-o',binary],{encoding:'utf8'});
    assert.equal(built.status,0,built.stderr);
    const names=['中文 应用','quote"','slash\\','dollar$','backtick`',"single'",'percent%f %u','line\n tab\t carriage\r'];
    for(let i=0;i<names.length;i++) {
      const directory=path.join(lab,names[i]);await mkdir(directory);
      const executable=path.join(directory,'Codex Usage Dashboard'),result=path.join(lab,`result-${i}.json`);
      await writeFile(executable,'#!/usr/bin/python3\nimport os,sys,json\nwith open(os.environ["COUSASH_ENTRY_RESULT"],"w") as f: json.dump(sys.argv,f)\n');await chmod(executable,0o755);
      const encoded=spawnSync(binary,[executable],{encoding:'utf8'});assert.equal(encoded.status,0,encoded.stderr);
      const entry=path.join(lab,`case-${i}.desktop`);await writeFile(entry,`[Desktop Entry]\nType=Application\nName=Cousash test\nExec=${encoded.stdout}\nTerminal=false\n`);
      const launched=spawnSync('gio',['launch',entry],{encoding:'utf8',env:{...process.env,COUSASH_ENTRY_RESULT:result}});assert.equal(launched.status,0,names[i]+': '+launched.stderr);
      let actual;for(let attempt=0;attempt<100;attempt++){try{actual=JSON.parse(await readFile(result,'utf8'));break;}catch{}await new Promise(r=>setTimeout(r,20));}
      assert.deepEqual(actual,[executable,'--background']);
    }
  } finally {
    assert(lab.startsWith(path.join(os.tmpdir(),'cousash-desktop-entry-')));await rm(lab,{recursive:true,force:true});
  }
});
