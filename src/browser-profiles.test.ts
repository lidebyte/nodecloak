import {beforeEach,describe,expect,it,vi} from 'vitest';
vi.mock('./bridge',()=>({native:false}));
const storage=new Map<string,string>();
beforeEach(()=>{vi.resetModules();storage.clear();vi.stubGlobal('localStorage',{getItem:(k:string)=>storage.get(k)??null,setItem:(k:string,v:string)=>storage.set(k,v)});});
describe('browser profiles',()=>{
  it('uses Firefox native concurrency preferences without enabling its automation interface',async()=>{
    const api=await import('./browser-profiles');const prefs=api.defaults().preferences;prefs.fingerprint=undefined;prefs.advanced.hardwareConcurrency=4;
    expect(api.requiresBrowserControl('firefox',prefs)).toBe(false);expect(api.requiresBrowserControl('chrome',prefs)).toBe(true);prefs.advanced.viewportWidth=1280;expect(api.requiresBrowserControl('firefox',prefs)).toBe(true);
  });
  it('requires bounded hardware and viewport values and preserves them through save/copy',async()=>{
    const api=await import('./browser-profiles');const d=api.defaults();d.name='hardware';Object.assign(d.preferences.advanced,{hardwareConcurrency:4,viewportWidth:1280,viewportHeight:800});
    const first=await api.saveProfile(d);const copy=await api.saveProfile({...d,name:'copy'},undefined,first.id);
    expect(copy.preferences.advanced).toMatchObject({hardwareConcurrency:4,viewportWidth:1280,viewportHeight:800});expect(api.needsRegionalControl(copy.preferences)).toBe(true);
    Object.assign(d.preferences.advanced,{hardwareConcurrency:3});expect(api.validateDraft(d)).toBeTruthy();Object.assign(d.preferences.advanced,{hardwareConcurrency:0,viewportWidth:10});expect(api.validateDraft(d)).toBeTruthy();
  });
  it('validates independent privacy settings for all engines and rejects unsupported canvas modes',async()=>{
    const api=await import('./browser-profiles');
    for(const browser of ['chrome','edge','firefox'] as const){const d=api.defaults();d.name='privacy';d.browser=browser;d.preferences.fingerprint=undefined;Object.assign(d.preferences.advanced,{doNotTrack:'on',webRtc:'public',blockWebgl:true,blockWebgpu:true,blockSpeech:true,blockBattery:browser==='firefox',canvasProtection:browser==='firefox'});expect(api.validateDraft(d)).toBeNull();Object.assign(d.preferences.advanced,{webRtc:'invalid'});expect(api.validateDraft(d)).toBeTruthy();}
  });
  it('uses browser names and migrates only generated default names',async()=>{
    const api=await import('./browser-profiles');
    const profiles=await api.listProfiles();expect(profiles.map(p=>p.name)).toEqual(['Chrome','Edge','Firefox']);
    profiles[0].name='默认 Chrome';profiles[1].name='My Edge';
    storage.set('claudedone.profiles-demo.v1',JSON.stringify([...profiles,{...profiles[0],id:'custom',legacy:false}]));
    vi.resetModules();
    expect((await (await import('./browser-profiles')).listProfiles()).map(p=>p.name)).toEqual(['Chrome','My Edge','Firefox','默认 Chrome']);
  });
  it('parses encoded credentials, IPv6 and protocol defaults',async()=>{
    const {parseProxyUri}=await import('./browser-profiles');
    expect(parseProxyUri('socks5://user:p%40ss@[::1]:1081')).toEqual({proxy:{mode:'socks5',host:'::1',port:1081,username:'user'},password:'p@ss'});
    expect(parseProxyUri('https://proxy.example').proxy.port).toBe(443);
    for(const uri of ['file:///tmp','http://host/path','socks5://host:70000','http://host?x=1'])expect(()=>parseProxyUri(uri)).toThrow();
  });
  it('keeps credentials out of persistent demo data and copies settings into a new identity',async()=>{
    const api=await import('./browser-profiles');const draft=api.defaults();draft.name='one';draft.proxy={mode:'http',host:'localhost',port:8080,username:'u'};draft.password='TEST-SECRET-DO-NOT-SAVE';
    const one=await api.saveProfile(draft);const two=await api.saveProfile({...draft,name:'two',password:''},undefined,one.id);
    expect(two.id).not.toBe(one.id);expect(two.hasPassword).toBe(true);expect([...storage.values()].join('')).not.toContain(draft.password);
    await api.startProfile(one.id);await api.saveProfile({...draft,preferences:{...draft.preferences,language:'en-GB,en'}},one.id);
    expect((await api.listProfiles()).find(p=>p.id===one.id)?.pendingRestart).toBe(true);
    expect((await api.listProfiles()).find(p=>p.id===two.id)?.running).toBe(false);
  });
  it('requires closing and trashing before purge, supports restore, and preserves empty registry',async()=>{
    const api=await import('./browser-profiles');const p=(await api.listProfiles())[0];await api.startProfile(p.id);
    await expect(api.deleteProfile(p.id)).rejects.toThrow();await api.closeProfile(p.id);await expect(api.deleteProfile(p.id,true)).rejects.toThrow();
    await api.deleteProfile(p.id);await api.restoreProfile(p.id);expect((await api.listProfiles())[0].deletedAt).toBeNull();
    for(const p of await api.listProfiles()){await api.deleteProfile(p.id);await api.deleteProfile(p.id,true);}vi.resetModules();
    expect(await (await import('./browser-profiles')).listProfiles()).toEqual([]);
  });
  it('rejects malformed ports, startup URLs and control characters in credentials',async()=>{
    const {defaults,validateDraft}=await import('./browser-profiles');const d=defaults();d.name='valid';expect(validateDraft(d)).toBeNull();
    d.preferences.startupUrl='javascript:alert(1)';expect(validateDraft(d)).toBeTruthy();d.preferences.startupUrl='about:blank';d.password='x\nheader';expect(validateDraft(d)).toBeTruthy();
  });
  it('migrates old profile preferences without changing existing values',async()=>{
    const api=await import('./browser-profiles');const old=api.defaults();old.name='existing';
    const {advanced:_,regional:__,...preferences}=old.preferences;
    storage.set('claudedone.profiles-demo.v1',JSON.stringify([{...old,preferences,id:'legacy-firefox',running:true}]));
    const [p]=await api.listProfiles();expect(p.preferences.advanced).toEqual(api.advancedDefaults());
    expect(p.preferences.regional).toEqual(api.regionalDefaults());
    expect(p.preferences.language).toBe(preferences.language);expect(p.running).toBe(false);
  });
  it('copies advanced settings independently, defers running edits and rejects unsupported browser options',async()=>{
    const api=await import('./browser-profiles');const d=api.defaults();d.name='strict';d.preferences.fingerprint=undefined;
    d.preferences.advanced.resistFingerprinting=true;d.preferences.advanced.notifications='block';
    const first=await api.saveProfile(d);const copy=await api.saveProfile({...d,name:'copy'},undefined,first.id);
    expect(copy.preferences.advanced).toEqual(d.preferences.advanced);
    await api.startProfile(first.id);d.preferences.advanced.blockImages=true;
    await api.saveProfile(d,first.id);
    expect((await api.listProfiles()).find(p=>p.id===first.id)?.pendingRestart).toBe(true);
    expect((await api.listProfiles()).find(p=>p.id===copy.id)?.preferences.advanced.blockImages).toBe(false);
    expect(api.validateDraft({...d,browser:'chrome'})).toContain('Firefox');
    expect(api.validateDraft({...d,preferences:{...d.preferences,advanced:{...d.preferences.advanced,notifications:'allow' as never}}})).toBeTruthy();
  });
  it('preserves explicitly enabled timezone and coordinate sources when loading saved profiles',async()=>{
    const api=await import('./browser-profiles');const d=api.defaults();d.name='existing';
    Object.assign(d.preferences.regional,{enabled:true,timezoneMode:'custom',timezone:'America/Los_Angeles',locationMode:'ip',latitude:35.68,longitude:139.69});
    d.preferences.advanced.location='allow';d.proxy.mode='direct';
    storage.set('claudedone.profiles-demo.v1',JSON.stringify([{...d,id:'legacy-firefox',running:false}]));
    const [p]=await api.listProfiles();
    expect(p.preferences.regional.timezoneMode).toBe('custom');
    expect(p.preferences.regional.locationMode).toBe('ip');
    expect(p.preferences.advanced.location).toBe('allow');
    expect(p.preferences.language).toBe('en-US,en');expect(p.proxy.mode).toBe('direct');
    expect(api.needsRegionalControl(p.preferences)).toBe(true);
  });
  it('supports native location permission without requiring an emulated position',async()=>{
    const api=await import('./browser-profiles');const d=api.defaults();d.name='native permissions';d.preferences.fingerprint=undefined;
    d.preferences.advanced.location='allow';
    expect(api.validateDraft(d)).toBeNull();expect(api.needsRegionalControl(d.preferences)).toBe(false);
    d.preferences.advanced.resistFingerprinting=true;expect(api.validateDraft(d)).toBeNull();
  });
  it('keeps IP language matching independent and defers running language edits',async()=>{
    const api=await import('./browser-profiles');const d=api.defaults();d.name='IP language';
    Object.assign(d.preferences.regional,{languageMode:'ip',timezoneMode:'ip',locationMode:'ip'});
    const first=await api.saveProfile(d);const copy=await api.saveProfile({...d,name:'copy'},undefined,first.id);
    expect(first.preferences.regional.timezoneMode).toBe('system');expect(first.preferences.regional.locationMode).toBe('system');
    await api.startProfile(first.id);d.preferences.regional.languageMode='custom';d.preferences.language='de-DE,de,en';
    await api.saveProfile(d,first.id);const profiles=await api.listProfiles();
    expect(profiles.find(p=>p.id===first.id)?.pendingRestart).toBe(true);
    expect(profiles.find(p=>p.id===copy.id)?.preferences.regional.languageMode).toBe('ip');
    expect((await api.matchRegion(d)).language).toBe('en-SG,en');
  });

});
