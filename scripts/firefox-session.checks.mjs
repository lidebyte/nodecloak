import {readFileSync} from 'node:fs';
import {strict as assert} from 'node:assert';
import {createHash} from 'node:crypto';
import test from 'node:test';
import vm from 'node:vm';
const source=readFileSync(new URL('../src-tauri/src/firefox_session.js',import.meta.url),'utf8');
const tick=()=>new Promise(resolve=>setImmediate(resolve));
const native={cookies:[{host:'example.invalid',name:'session',value:'saved',path:'/'}],windows:[{tabs:[{entries:[{url:'https://example.invalid/old'},{url:'https://example.invalid/current'}],index:2,formdata:{id:{draft:'saved draft'}},scroll:'0,700'}],selected:1}]};
function fixture(data,{failedSave=false,redirect=false,changedRead=false,legacy=false}={}){
 const writes=[],restored=[],cookies=[],order=[],listeners=new Map();let disk={windows:[{tabs:[{entries:[{url:'about:blank'}]}]}]},reads=0;
 const tab=(url,id)=>({linkedBrowser:{currentURI:{spec:url},browsingContext:{id}},_tPos:id-1});
 const helper=tab('gate',1),other=tab('https://user.invalid/',2),tabs=[helper,other];
 const browser={selectedBrowser:helper.linkedBrowser,selectedTab:helper,tabs,
  getTabForBrowser:browser=>tabs.find(t=>t.linkedBrowser===browser),
  selectTabAtIndex:index=>{browser.selectedTab=tabs[index];browser.selectedBrowser=tabs[index].linkedBrowser},
  removeTab:tab=>tabs.splice(tabs.indexOf(tab),1)};
 const win={gBrowserInit:{delayedStartupFinished:true},addEventListener:(type,fn)=>listeners.set(type,fn),gBrowser:browser};
 const SessionStore={isFormatVersionCompatible:version=>version[0]==='sessionrestore'&&version[1]<=1,setTabState(target,value){assert.equal(target,helper);order.push('tab');const state=JSON.parse(value);restored.push({tab:state});helper.linkedBrowser.currentURI.spec=state.entries[(state.index||1)-1].url;},setWindowState(window,value,overwrite){assert.equal(window,win);assert.equal(overwrite,false);order.push('tabs');const state=JSON.parse(value);restored.push(state);tabs.push(...state.windows[0].tabs.map((t,i)=>tab(t.entries[(t.index||1)-1].url,i+tabs.length+1)));listeners.get('SSWindowRestored')?.();}};
 const context=vm.createContext({testWindow:win,Cu:{waiveXrays:v=>v},IOUtils:{readJSON:async path=>path==='recovery'?disk:(path==='corrupt'?Promise.reject(Error('bad file')):data),read:async()=>Buffer.from(JSON.stringify(disk)+(changedRead&&++reads>1?'changed':'')),writeJSON:async(path,value)=>writes.push({path,value})},
 ChromeUtils:{importESModule:path=>{if(legacy&&path.startsWith('moz-src:'))throw Error('unsupported path');return path.includes('SessionCookies')?{SessionCookies:{restore:value=>{order.push('cookies');cookies.push(...value)}}}:path.includes('SessionSaver')?{SessionSaver:{run:async()=>{if(!failedSave)disk=redirect?{windows:[{tabs:[{entries:[{url:'https://example.invalid/redirected'}]}]}]}:data;}}}:path.includes('SessionFile')?{SessionFile:{Paths:{recovery:'recovery'}}}:{SessionStore}}},
 Cc:{'@mozilla.org/security/hash;1':{createInstance(){let hash;return{SHA256:1,init(){hash=createHash('sha256')},update(bytes){hash.update(bytes)},finish(){return hash.digest('latin1')}}}}},Ci:{nsICryptoHash:{}},Services:{tm:{dispatchToMainThread:setImmediate},obs:{addObserver(){},removeObserver(){}}}});
 vm.runInContext(source,context);
 const prepare=async(paths=['snapshot'])=>{context.nodecloakPrepareSavedSession(paths,'receipt','token');await tick();vm.runInContext("Object.assign(globalThis[Symbol.for('nodecloak.saved-session')],{window:testWindow,browser:testWindow.gBrowser.selectedBrowser,gate:'gate'})",context);};
 const settle=async()=>{for(let i=0;i<10&&context.nodecloakSavedSessionStatus()==='restoring';i++)await tick();};
 return {context,writes,restored,tabs,other,cookies,order,prepare,settle};
}
test('appends native history/forms/scroll, restores session cookies first and preserves newly opened user tabs',async()=>{
 const {context,writes,restored,tabs,other,cookies,order,prepare,settle}=fixture(native);await prepare();
 assert.equal(context.nodecloakSavedSessionStatus(),'ready');assert.equal(context.nodecloakRestoreSavedSession(),'restoring');await settle();
 assert.deepEqual(restored,[{tab:native.windows[0].tabs[0]}]);assert.equal(context.nodecloakSavedSessionStatus(),'restored');
 assert.deepEqual(cookies,native.cookies);assert.deepEqual(order,['cookies','tab']);
 assert.equal(writes.length,1);assert.equal(writes[0].value.sha256,createHash('sha256').update(JSON.stringify(native)).digest('hex'));
 assert.ok(tabs.includes(other));assert.equal(tabs.length,2);
 assert.throws(()=>context.nodecloakRestoreSavedSession(),/SESSION_NOT_READY/);
});
test('failed native save, redirects and an autosave during readback retain originals without failing working pages',async()=>{
 for(const options of [{failedSave:true},{redirect:true},{changedRead:true}]){
  const {context,writes,prepare,settle}=fixture(native,options);await prepare();context.nodecloakRestoreSavedSession();await settle();
  assert.equal(context.nodecloakSavedSessionStatus(),'restored');assert.equal(writes.length,1);assert.equal(writes[0].value.status,'retained');assert.equal(writes[0].value.sha256,undefined);
 }
});
test('tries native backups and supports legacy module paths',async()=>{
 const {context,prepare,settle}=fixture(native,{legacy:true});await prepare(['corrupt','snapshot']);assert.equal(context.nodecloakSavedSessionStatus(),'ready');context.nodecloakRestoreSavedSession();await settle();assert.equal(context.nodecloakSavedSessionStatus(),'restored');
});
test('invalid or private session state cannot overwrite the current browser',async()=>{
 for(const data of [{windows:[]},{windows:[{isPrivate:true,tabs:[{}]}]},{windows:[{tabs:[]}]},{...native,version:['sessionrestore',999]}]){
  const {context,restored,writes,prepare}=fixture(data);await prepare();
  assert.equal(context.nodecloakSavedSessionStatus(),'failed');assert.throws(()=>context.nodecloakRestoreSavedSession(),/SESSION_NOT_READY/);
  assert.equal(restored.length,0);assert.equal(writes.length,0);
 }
});
