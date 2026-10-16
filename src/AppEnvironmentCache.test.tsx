// @vitest-environment jsdom
// Opening environment/history views must not trigger network scans.
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import App from './App';
import {LanguageProvider} from './LanguageControl';
import {languageStorageKey} from './i18n';
import * as bridge from './bridge';
import * as profiles from './browser-profiles';
import type {Scan,RepairRecord} from './domain';
vi.mock('./browser-site-catalog',async original=>({...await original<object>(),useBrowserSiteCatalog:()=>({catalog:{revision:0,groups:[],announcements:[]}})}));
vi.mock('./AppUpdates',()=>({default:()=>null}));
let host:HTMLDivElement,root:Root;
const snapshot:Scan={platform:'Saved OS',browser:'chrome',browserAvailable:true,profilePath:'cached',checkedAt:'2026-10-16T12:00:00Z',checks:[{id:'timezone',status:'healthy',value:'Saved timezone',detail:'Stored observation',fixable:false}],ip:null,location:null,latency:null,cliInstalled:false};
const click=async(name:string)=>{const button=Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.trim()===name);expect(button,name).toBeDefined();await act(async()=>{button!.click();});};
beforeEach(()=>{localStorage.clear();localStorage.setItem(languageStorageKey,'zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});vi.stubGlobal('matchMedia',()=>({matches:false,addEventListener:()=>{},removeEventListener:()=>{}}));HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};host=document.createElement('div');document.body.append(host);root=createRoot(host);vi.spyOn(bridge,'scanEnvironment').mockResolvedValue(snapshot);vi.spyOn(bridge,'getHistory').mockResolvedValue([]);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.restoreAllMocks();vi.unstubAllGlobals();bridge.setActiveProfile(null);});
it('opens computer and overview without scans, then restores saved results on revisit and app reopening',async()=>{
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));
 await click('电脑环境');expect(bridge.scanEnvironment).not.toHaveBeenCalled();expect(host.textContent).toContain('尚无检测结果');
 await click('重新检测');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);expect(host.textContent).toContain('Saved timezone');
 await click('环境概览');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);expect(host.textContent).not.toContain('Saved timezone');
 await click('电脑环境');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);expect(host.textContent).toContain('Saved timezone');
 await act(async()=>root.unmount());root=createRoot(host);await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await click('电脑环境');
 expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);expect(host.textContent).toContain('Saved timezone');
});
it('opens and refreshes repair history without running environment detection',async()=>{
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));
 await click('修复记录');expect(bridge.scanEnvironment).not.toHaveBeenCalled();expect(bridge.getHistory).toHaveBeenCalledTimes(1);
 await click('浏览器副本');await click('修复记录');expect(bridge.scanEnvironment).not.toHaveBeenCalled();expect(bridge.getHistory).toHaveBeenCalledTimes(1);
 await click('刷新记录');expect(bridge.getHistory).toHaveBeenCalledTimes(2);expect(bridge.scanEnvironment).not.toHaveBeenCalled();
});
it('keeps the last successful result visible when a manual recheck fails',async()=>{
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await click('电脑环境');await click('重新检测');
 vi.mocked(bridge.scanEnvironment).mockRejectedValueOnce(new Error('Offline check'));
 await click('重新检测');expect(host.textContent).toContain('Saved timezone');expect(host.textContent).toContain('上次检测：');expect(host.querySelector('[role="alert"]')?.textContent).toContain('Offline check');
 await click('浏览器副本');await click('电脑环境');expect(host.textContent).toContain('Saved timezone');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(2);
});
it('retains computer results after switching to a different browser profile',async()=>{
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await click('电脑环境');await click('重新检测');await click('浏览器副本');
 const edge=Array.from(host.querySelectorAll<HTMLButtonElement>('.profile-name-button')).find(b=>b.textContent?.startsWith('Edge'))!;
 await act(async()=>edge.click());await click('电脑环境');expect(host.textContent).toContain('Saved timezone');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);
});
it('restores each profile separately and prevents late history from replacing another scope',async()=>{
 const original=(await profiles.listProfiles())[0];
 vi.spyOn(profiles,'listProfiles').mockResolvedValue([{...original,id:'first',name:'First'},{...original,id:'second',name:'Second'}]);
 vi.mocked(bridge.scanEnvironment).mockResolvedValue({...snapshot,checks:[{id:'language',status:'healthy',value:'First profile language',detail:'Saved language',fixable:false}]});
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));
 const openProfile=async(name:string)=>{const button=Array.from(host.querySelectorAll<HTMLButtonElement>('.profile-name-button')).find(b=>b.textContent?.startsWith(name));expect(button).toBeDefined();await act(async()=>button!.click());};
 await openProfile('First');await click('重新检测');expect(host.textContent).toContain('First profile language');
 await click('浏览器副本');await openProfile('Second');expect(host.textContent).not.toContain('First profile language');
 await click('浏览器副本');await openProfile('First');expect(host.textContent).toContain('First profile language');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);
 await click('浏览器副本');await openProfile('Second');
 let finish!:(records:RepairRecord[])=>void;
 vi.mocked(bridge.getHistory).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;})).mockResolvedValueOnce([]);
 await click('修复记录');expect(host.textContent).toContain('正在读取修复记录');
 await click('电脑环境');await click('修复记录');
 await act(async()=>finish([{id:'old',profileId:'second',browser:'chrome',createdAt:snapshot.checkedAt,itemId:'language',status:'applied',message:'Second profile repair',changes:[]}]));
 expect(host.textContent).not.toContain('Second profile repair');expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);
});
it('uses the latest manual detection for browser availability',async()=>{
 const target=(await profiles.listProfiles())[0];vi.spyOn(profiles,'listProfiles').mockResolvedValue([{...target,browserAvailable:false}]);
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await click('环境概览');
 const launch=()=>Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.trim()==='打开浏览器')!;
 expect(launch().disabled).toBe(true);await click('重新检测');expect(launch().disabled).toBe(false);
 vi.mocked(bridge.scanEnvironment).mockResolvedValueOnce({...snapshot,browserAvailable:false});await click('重新检测');expect(launch().disabled).toBe(true);
});
it('revalidates the selected profile after it was deleted from the list',async()=>{
 const target=(await profiles.listProfiles())[0];const listing=vi.spyOn(profiles,'listProfiles').mockResolvedValue([{...target,id:'first',name:'First'}]);
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await click('环境概览');expect(host.querySelector('.active-profile-bar')?.textContent).toContain('First');
 await click('浏览器副本');listing.mockResolvedValue([{...target,id:'second',name:'Second'}]);await click('环境概览');
 expect(host.querySelector('.active-profile-bar')?.textContent).toContain('Second');expect(host.querySelector('.active-profile-bar')?.textContent).not.toContain('First');expect(bridge.scanEnvironment).not.toHaveBeenCalled();
});
it('refreshes cached aggregate history after restoring a profile setting',async()=>{
 const target=(await profiles.listProfiles())[0];vi.spyOn(profiles,'listProfiles').mockResolvedValue([target]);
 let records:RepairRecord[]=[{id:'repair',profileId:target.id,browser:'chrome',createdAt:snapshot.checkedAt,itemId:'language',status:'applied',message:'Profile language changed',changes:[]}];
 vi.mocked(bridge.getHistory).mockImplementation(async()=>records);
 vi.spyOn(bridge,'undoRepair').mockImplementation(async()=>{records=[{...records[0],status:'undone',message:'Profile language restored'}];});
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await click('电脑环境');await click('修复记录');expect(host.textContent).toContain('Profile language changed');
 await click('环境概览');await click('修复记录');await click('恢复原值');
 await act(async()=>host.querySelector<HTMLButtonElement>('dialog.modal[open] .modal-actions .primary')!.click());
 await click('电脑环境');await click('修复记录');expect(host.querySelector('.history-record')?.textContent).toContain('Profile language restored');expect(host.querySelector('.record-status')?.textContent).toBe('已撤销');
 expect(bridge.scanEnvironment).toHaveBeenCalledTimes(1);
});
