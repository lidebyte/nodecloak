import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import assert from 'node:assert/strict';
import {test} from 'node:test';

const fingerprint={version:1,seed:'37'.repeat(32),enabled:true,canvasNoise:true,fontNoise:true,webglNoise:true,audioNoise:true,mediaNoise:true};
const source=()=>readFileSync(new URL('./background.js',import.meta.url),'utf8');
async function boot(response){
 let intercept,registered,requests=0;
 let respond;
 const native=new Promise(resolve=>{respond=resolve;});
 const browser={
  runtime:{sendNativeMessage:async(name,message)=>{assert.equal(name,'com.nodecloak.fingerprint');assert.deepEqual(JSON.parse(JSON.stringify(message)),{type:'get_settings'});requests++;return native;}},
  contentScripts:{register:async options=>{registered=options;return {unregister:async()=>{}};}},
  webRequest:{onBeforeRequest:{addListener:listener=>{intercept=listener;}}},
 };
 runInNewContext(source(),{browser,nodecloakFingerprint:function(config){globalThis.applied=config;},setTimeout,clearTimeout,console:{error(){}}});
 assert.equal(typeof intercept,'function');
 let released=false;
 const request=intercept({url:'https://example.test/',type:'main_frame'}).then(result=>{released=true;return result;});
 await new Promise(resolve=>setImmediate(resolve));
 assert.equal(released,false,'navigation must wait for profile settings');
 respond(response);
 return {result:await request,registered,requests};
}
test('registers validated profile settings before releasing the first document',async()=>{
 const result=await boot({fingerprint});
 assert.deepEqual(JSON.parse(JSON.stringify(result.result)),{});
 assert.equal(result.requests,1);
 assert.equal(result.registered.world,'MAIN');
 assert.equal(result.registered.runAt,'document_start');
 assert.equal(result.registered.allFrames,true);
 assert.equal(result.registered.matchAboutBlank,true);
 const context={};runInNewContext(result.registered.js[0].code,context);
 assert.deepEqual(JSON.parse(JSON.stringify(context.applied)),fingerprint);
});
test('missing, disabled, future or malformed settings never release an unprotected document',async()=>{
 for(const value of [null,{}, {fingerprint:{...fingerprint,enabled:false}},{fingerprint:{...fingerprint,version:2}},{fingerprint:{...fingerprint,seed:'invalid'}},{fingerprint:{...fingerprint,seed:[fingerprint.seed]}},{fingerprint:{...fingerprint,canvasNoise:'true'}},{fingerprint:{...fingerprint,other:'unexpected'}}]){
  const result=await boot(value);
  assert.deepEqual(JSON.parse(JSON.stringify(result.result)),{cancel:true});
  assert.equal(result.registered,undefined);
 }
});
