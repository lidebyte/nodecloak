import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source=readFileSync(new URL('../src-tauri/src/cloud/session_codec.js',import.meta.url),'utf8');
const sandbox={Blob,File,ArrayBuffer,Uint8Array,Uint16Array,Uint32Array,Uint8ClampedArray,Int8Array,Int16Array,Int32Array,Float32Array,Float64Array,BigInt64Array,BigUint64Array,DataView,Date,Map,Set,RegExp,Object,Number,BigInt,Array,String,JSON,WeakMap,Math,crypto,atob,btoa};vm.createContext(sandbox);vm.runInContext(source,sandbox);const codec=sandbox.__ncSessionCodec;
test('structured-clone values preserve cycles, binary, maps, dates, undefined and bigint',async()=>{const value={bytes:new Uint8Array([1,2,255]),date:new Date('2026-01-01T00:00:00Z'),n:12n,missing:undefined,set:new Set(['a','b']),map:new Map([['a',NaN]]),blob:new Blob(['token'],{type:'text/plain'})};value.self=value;const clone=await codec.unpack(await codec.pack(value));assert.equal(clone.self,clone);assert.deepEqual([...clone.bytes],[1,2,255]);assert.equal(clone.date.toISOString(),'2026-01-01T00:00:00.000Z');assert.equal(clone.n,12n);assert.equal(clone.missing,undefined);assert.deepEqual([...clone.set],['a','b']);assert(Number.isNaN(clone.map.get('a')));assert.equal(await clone.blob.text(),'token');});
test('untrusted graphs cannot instantiate code or pollute prototypes',async()=>{await assert.rejects(()=>codec.unpack({root:{ref:0},nodes:[{type:'Function',source:'process.exit()'}]}));const input=JSON.parse('{"__proto__":{"polluted":true},"constructor":"data"}');const result=await codec.unpack(await codec.pack(input));assert.equal(Object.prototype.polluted,undefined);assert.equal(result.__proto__.polluted,true);assert.equal(result.constructor,'data');});
test('non-exportable cryptographic device keys fail explicitly',async()=>{const key=await crypto.subtle.generateKey({name:'AES-GCM',length:256},false,['encrypt','decrypt']);await assert.rejects(()=>codec.pack(key),/non-exportable/);});
test('invalid references and oversized arrays fail before import',async()=>{await assert.rejects(()=>codec.unpack({root:{ref:300},nodes:[]}));await assert.rejects(()=>codec.pack('x'.repeat(65<<20)));await assert.rejects(()=>codec.unpack({root:{ref:0},nodes:[{type:'Array',values:new Array(60000).fill(null)}]}));});
test('large single storage values and aggregate exports can exceed the former 8 MiB cap',async()=>{
 const value='x'.repeat(9<<20);assert.equal(await codec.unpack(await codec.pack(value)),value);
 const scope={...sandbox,localStorage:{login:value},indexedDB:{databases:async()=>[]}};
 vm.createContext(scope);vm.runInContext(source,scope);
 const state=await scope.__ncSessionCodec.exportOrigin();assert.equal(state.local.length,1);assert.equal(state.local[0][0],'login');assert.equal(state.local[0][1],value);
});
test('UTF-8 aggregate limits are checked before any storage mutation',async()=>{
 let mutations=0;
 const scope={...sandbox,localStorage:{clear(){mutations++;},setItem(){mutations++;}},indexedDB:{databases:async()=>[]}};
 vm.createContext(scope);vm.runInContext(source,scope);
 // Under 64 MiB in UTF-16 code units, over 64 MiB as serialized UTF-8.
 const state={local:Array.from({length:23},(_,i)=>['key'+i,'汉'.repeat(1<<20)]),databases:[]};
 await assert.rejects(()=>scope.__ncSessionCodec.importOrigin(state),/NC_LIMIT/);assert.equal(mutations,0);
});
