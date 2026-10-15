import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {randomUUID} from 'node:crypto';

test('snapshot references cannot operate a replaced document or previous snapshot',()=>{
  const source=fs.readFileSync(new URL('../src-tauri/src/mcp/page.js',import.meta.url),'utf8');
  const button={tagName:'BUTTON',innerText:'Save',getAttribute:()=>null,getBoundingClientRect:()=>({width:50,height:20}),click(){this.clicked=true;},isConnected:true};
  const document={body:{innerText:'Hello'},title:'Fixture',querySelectorAll:()=>[button]};
  // Opaque about:blank and insecure documents do not provide crypto.randomUUID.
  const ctx=vm.createContext({document,location:{href:'about:blank'},performance:{timeOrigin:1},getComputedStyle:()=>({display:'block',visibility:'visible'}),Map,JSON});
  const run=args=>vm.runInContext(`(${source})(${JSON.stringify(args.action==='snapshot'?{...args,_new_snapshot_id:randomUUID()}:args)})`,ctx);
  const a=run({action:'snapshot'}); assert.equal(a.elements[0].name,'Save');
  run({action:'wait',text:'Hello'});
  const preview=run({action:'describe',snapshot_id:a.snapshot_id,element_ref:a.elements[0].ref});
  assert.equal(preview.name,'Save');assert.equal(preview.epoch,1);
  const b=run({action:'snapshot'});
  assert.throws(()=>run({action:'click',snapshot_id:a.snapshot_id,element_ref:a.elements[0].ref}),/STALE/);
  run({action:'click',snapshot_id:b.snapshot_id,element_ref:b.elements[0].ref});assert.equal(button.clicked,true);
  ctx.document={...document};
  assert.throws(()=>run({action:'click',snapshot_id:b.snapshot_id,element_ref:b.elements[0].ref}),/STALE/);
});
