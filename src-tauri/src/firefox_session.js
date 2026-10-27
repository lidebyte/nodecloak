// Runs in Firefox's parent chrome context only, never in website documents.
function nodecloakPrepareSavedSession(snapshotPaths, receiptPath, token) {
  const state = globalThis[Symbol.for('nodecloak.saved-session')] = {status:'pending',receiptPath,token};
  (async () => {
    for (const path of snapshotPaths) {
      try {
        const value = await IOUtils.readJSON(path, {decompress:true, decrypt:'sessionstore'});
        if (!value || !Array.isArray(value.windows) || !value.windows.length ||
            value.windows.some(win => win.isPrivate || !Array.isArray(win.tabs) || !win.tabs.length ||
              win.tabs.some(tab => !Array.isArray(tab.entries) || !tab.entries.length))) continue;
        const {SessionStore}=nodecloakSessionModule('SessionStore');
        if(typeof SessionStore.isFormatVersionCompatible==='function' &&
           !SessionStore.isFormatVersionCompatible(value.version||['sessionrestore',0])) continue;
        state.data = value; state.status = 'ready'; return;
      } catch {}
    }
    state.status = 'failed';
  })();
  return 'pending';
}

function nodecloakSavedSessionStatus() {
  return globalThis[Symbol.for('nodecloak.saved-session')]?.status || 'missing';
}

function nodecloakSessionCoverage(value) {
  return (value?.windows || []).flatMap(win => win.tabs || []).map(tab =>
    JSON.stringify((tab.entries || []).map(entry => entry.url)));
}
function nodecloakHasSavedTabs(expected, actual) {
  const remaining = nodecloakSessionCoverage(actual);
  return nodecloakSessionCoverage(expected).every(key => {
    const index = remaining.indexOf(key);
    if (index < 0) return false;
    remaining.splice(index,1); return true;
  });
}
function nodecloakSessionModule(name) {
  try {return ChromeUtils.importESModule('moz-src:///browser/components/sessionstore/'+name+'.sys.mjs');}
  catch {return ChromeUtils.importESModule('resource:///modules/sessionstore/'+name+'.sys.mjs');}
}

function nodecloakWriteSessionReceipt(state,value) {
  return IOUtils.writeJSON(state.receiptPath,{schema:1,token:state.token,...value},{flush:true});
}

function nodecloakRestoreSavedSession(restorePreferences=[]) {
  const state = globalThis[Symbol.for('nodecloak.saved-session')];
  if (state?.status !== 'ready' || !state.window) throw Error('SESSION_NOT_READY');
  const {SessionStore} = nodecloakSessionModule('SessionStore');
  const {SessionSaver} = nodecloakSessionModule('SessionSaver');
  state.status = 'restoring';
  (async () => {
    try {
      state.phase='restore-windows';
      const {SessionCookies}=nodecloakSessionModule('SessionCookies');
      SessionCookies.restore(state.data.cookies||[]);
      for (let i=0;i<state.data.windows.length;i++) {
        const win = Cu.waiveXrays(i===0 ? state.window : state.window.OpenBrowserWindow());
        if (!win.gBrowserInit?.delayedStartupFinished) {
          await new Promise(resolve => {
            const observer={observe(subject) {if(subject===win){Services.obs.removeObserver(observer,'browser-delayed-startup-finished');resolve();}}};
            Services.obs.addObserver(observer,'browser-delayed-startup-finished');
          });
        }
        const helper=i===0 ? state.browser : win.gBrowser.selectedBrowser;
        let target=win.gBrowser.getTabForBrowser(helper);
        if(!target || (i===0 && helper.currentURI.spec!==state.gate)) throw Error('STARTUP_TAB_CHANGED');
        const savedWindow=state.data.windows[i];
        const selected=Math.max(0,Math.min(savedWindow.tabs.length-1,(savedWindow.selected||1)-1));
        // Restore the saved active tab directly into our selected gate tab.
        // The browser's native tab-state API restores its history/form/scroll
        // immediately, without depending on selecting an uninserted lazy tab.
        SessionStore.setTabState(target,JSON.stringify(savedWindow.tabs[selected]));
        const remaining=savedWindow.tabs.filter((_,index)=>index!==selected);
        if(remaining.length){
          const ready=new Promise(resolve=>win.addEventListener('SSWindowRestored',resolve,{once:true}));
          SessionStore.setWindowState(win,JSON.stringify({windows:[{...savedWindow,tabs:remaining}]}),false);
          await ready;
        }
      }
      state.phase='save';
      state.pagesRestored=true;
      await SessionSaver.run();
      // Native save promises also resolve on some write failures. Read back
      // the actual file, prove tab coverage, and bind the receipt to its bytes.
      const {SessionFile}=nodecloakSessionModule('SessionFile');
      state.phase='readback';
      const path=SessionFile.Paths.recovery;
      const bytes=await IOUtils.read(path);
      const actual=await IOUtils.readJSON(path,{decompress:true,decrypt:'sessionstore'});
      if(!nodecloakHasSavedTabs(state.data,actual)) throw Error('SAVED_SESSION_NOT_PERSISTED');
      const after=await IOUtils.read(path);
      if(bytes.length!==after.length || bytes.some((value,index)=>value!==after[index])) throw Error('SAVED_SESSION_CHANGED_DURING_READBACK');
      state.phase='digest';
      const hash=Cc['@mozilla.org/security/hash;1'].createInstance(Ci.nsICryptoHash);
      hash.init(hash.SHA256);hash.update(bytes,bytes.length);
      const digest=Array.from(hash.finish(false),c=>c.charCodeAt(0).toString(16).padStart(2,'0')).join('');
      await nodecloakWriteSessionReceipt(state,{file:'sessionstore-backups/recovery.jsonlz4',sha256:digest});
      state.status='restored';delete state.data;
    } catch (error) {
      state.status=state.pagesRestored ? 'restored' : 'failed';
      await nodecloakWriteSessionReceipt(state,{status:state.pagesRestored?'retained':'failed',stage:state.phase,reason:error.name});
    } finally {
      for (const [name,value] of restorePreferences) {
        if(value===null) Services.prefs.clearUserPref(name);
        else Services.prefs.setBoolPref(name,value);
      }
      if(state.status==='restored') {
        delete state.data;
        delete state.window[Symbol.for('nodecloak.saved-session-sandbox')];
      }
    }
  })();
  return 'restoring';
}

function nodecloakSessionReadinessPage(url,win) {
  if(!win) throw Error('NATIVE_REGION_STARTUP_PENDING');
  const state=globalThis[Symbol.for('nodecloak.saved-session')];
  state.window=win;state.browser=win.gBrowser.selectedBrowser;state.gate=url;
  state.browser.loadURI(Services.io.newURI(url),{triggeringPrincipal:Services.scriptSecurityManager.getSystemPrincipal()});
  return 'waiting';
}
