import {readFileSync} from 'node:fs';
import {strict as assert} from 'node:assert';
import test from 'node:test';
import vm from 'node:vm';

const source=readFileSync(new URL('../src-tauri/src/firefox_region.js',import.meta.url),'utf8');
test('an early debugger connection waits for browser chrome before mutation',()=>{
 let writes=0;
 const context=vm.createContext({Services:{prefs:{setCharPref(){writes++;}},wm:{getMostRecentWindow:()=>({}),getEnumerator:()=>[{}]}}});
 vm.runInContext(source,context);
 assert.equal(context.nodecloakConfigureRegion({locale:'de-DE',timezone:'America/Los_Angeles',width:0}),'pending');
 assert.equal(writes,0);
});
test('missing native region APIs fail before preference mutation or fake JS properties',()=>{
 const writes=[];
 const bc={isContent:true,parent:null,isDiscarded:false}; // Older browser, no native setters.
 const win={gBrowserInit:{delayedStartupFinished:true,uriToLoadPromise:'about:blank'},gBrowser:{browsers:[{browsingContext:bc}],tabContainer:{addEventListener(){}}},addEventListener(){}};
 const context=vm.createContext({Services:{prefs:{setCharPref:(...v)=>writes.push(v),setBoolPref:(...v)=>writes.push(v)},
   obs:{addObserver(){}},wm:{getMostRecentWindow:()=>win,getEnumerator:()=>[win]}}});
 vm.runInContext(source,context);
 assert.throws(()=>context.nodecloakConfigureRegion({language:'de-DE,de',locale:'de-DE',timezone:'America/Los_Angeles',geoUri:null,width:0,height:0}),/NATIVE_REGION_UNSUPPORTED/);
 assert.equal(writes.length,0);
 assert.equal(Object.hasOwn(bc,'timezoneOverride'),false);
 assert.equal(Object.hasOwn(bc,'languageOverride'),false);
});
test('new popup contexts receive native region before their embedder exists',()=>{
 const observers=new Map();const sized={};
 class NativeContext {
   isContent=true;parent=null;isDiscarded=false;_language='';_timezone='';_dppx=0;
   get languageOverride(){return this._language;}set languageOverride(v){this._language=v;}
   get timezoneOverride(){return this._timezone;}set timezoneOverride(v){this._timezone=v;}
   get overrideDPPX(){return this._dppx;}set overrideDPPX(v){this._dppx=v;}
 }
 const browser={browsingContext:new NativeContext(),style:{setProperty:(k,v)=>sized[k]=v},ownerDocument:{synchronouslyUpdateRemoteBrowserDimensions(){}}};
 browser.browsingContext.embedderElement=browser;
 const win={gBrowserInit:{delayedStartupFinished:true,uriToLoadPromise:'about:blank'},gBrowser:{browsers:[browser],tabContainer:{addEventListener(){}}},addEventListener(){}};
 const context=vm.createContext({Services:{prefs:{setCharPref(){},setBoolPref(){}},
   obs:{addObserver:(o,t)=>observers.set(t,o)},wm:{getMostRecentWindow:()=>win,getEnumerator:()=>[win]}}});
 vm.runInContext(source,context);
 assert.equal(context.nodecloakConfigureRegion({language:'de-DE,de',locale:'de-DE',timezone:'America/Los_Angeles',geoUri:null,width:1000,height:700}),'configured');
 const popup=new NativeContext();popup.embedderElement=null;
 observers.get('browsing-context-attached').observe(popup);
 assert.equal(popup.timezoneOverride,'America/Los_Angeles');
 assert.equal(popup.languageOverride,'de-DE');
 assert.equal(popup.overrideDPPX,1);
 popup.embedderElement=browser;
 observers.get('browsing-context-did-set-embedder').observe(popup);
 assert.equal(sized.width,'1000px');assert.equal(sized.height,'700px');
});

test('regional handoff waits until Firefox has consumed its delayed initial URI',async()=>{
 const loads=[];const queued=[];let resolveInitial;
 const initial=new Promise(resolve=>resolveInitial=resolve);
 class NativeContext {
   isContent=true;parent=null;isDiscarded=false;_language='';
   get languageOverride(){return this._language;}set languageOverride(v){this._language=v;}
 }
 const browser={browsingContext:new NativeContext(),loadURI:uri=>loads.push(uri),focus(){}};
 const win={gBrowserInit:{delayedStartupFinished:false,uriToLoadPromise:initial},
   gBrowser:{browsers:[browser],selectedBrowser:browser,tabContainer:{addEventListener(){}}},addEventListener(){},focus(){}};
 const services={wm:{getMostRecentWindow:()=>win,getEnumerator:()=>[win]},
   tm:{dispatchToMainThread:callback=>queued.push(callback)},
   io:{newURI:v=>v},scriptSecurityManager:{getSystemPrincipal:()=>({})},
   obs:{addObserver(){}},prefs:{clearUserPref(){},setBoolPref(){},setCharPref(){}}};
 const api={useDistinctSystemPrincipalLoader:()=>({require:()=>({DevToolsServer:{closeAllSocketListeners(){}}})}),releaseDistinctSystemPrincipalLoader(){}};
 const context=vm.createContext({Services:services,ChromeUtils:{importESModule:()=>api}});
 vm.runInContext(source,context);
 const config={language:'de-DE',locale:'de-DE',timezone:null,geoUri:null,width:0};
 assert.equal(context.nodecloakConfigureRegion(config), 'pending');
 assert.equal(queued.length,0);
 win.gBrowserInit.delayedStartupFinished=true;
 // Firefox has marked chrome ready but is still resolving its command-line URI.
 initial.then(()=>queued.push(()=>loads.push('about:blank')));
 assert.equal(context.nodecloakConfigureRegion(config), 'pending');
 assert.equal(queued.length,0);
 resolveInitial();await new Promise(resolve=>setImmediate(resolve));
 while(queued.length)queued.shift()();
 assert.deepEqual(loads,['about:blank']);
 // A finish must now target the settled initial browser, even if selection changes.
 context.nodecloakFinishRegion('http://127.0.0.1/start',[]);
 win.gBrowser.selectedBrowser={loadURI:()=>loads.push('wrong-tab'),focus(){}};
 while(queued.length)queued.shift()();
 assert.deepEqual(loads,['about:blank','http://127.0.0.1/start']);
});
