// @vitest-environment jsdom
import {act,useState} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import {AccountDialog,CloudSettings,type Controller} from './AccountWorkspace';
import * as profiles from './browser-profiles';
import {setLanguage} from './i18n';
import type {CloudConflict} from './cloud';
vi.mock('./bridge',()=>({native:false}));
let host:HTMLDivElement,root:Root;
beforeEach(()=>{vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});localStorage.clear();setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});Element.prototype.scrollIntoView=()=>{};HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.restoreAllMocks();vi.unstubAllGlobals();});
const conflict=(id:string):CloudConflict=>({localId:id,cloudId:'cloud-'+id,name:'Test '+id,kind:'config',fields:['deletion_vs_edit'],remoteRevision:2});
const button=(text:string)=>Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()===text);
function controller(conflicts:CloudConflict[]):Controller{return {status:{account:{id:'test',name:'Test',email:'test@example.com',avatar:''},unlocked:true,recoverySaved:true,setupComplete:true,conflicts},panel:'sync',busy:false,error:'',googlePending:false,beginGoogle:async()=>null,setError:()=>{},setPanel:()=>{},open:vi.fn(),act:vi.fn(async()=>null),reload:vi.fn(async()=>{})};}

it.each(['云端已删除此副本，本机配置和登录数据已保留。请在管理同步的待处理冲突中选择保留本地，或取消云端关联后再启动。','云端配置已变化，请先同步副本配置后再打开浏览器'])('offers targeted recovery for launch failure: %s',async message=>{
 const target=(await profiles.listProfiles())[0];
 vi.spyOn(profiles,'startProfile').mockRejectedValue(new Error(message));
 const open=vi.fn(),refresh=vi.fn();
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} onResolveSync={open} onSyncStatusChange={refresh}/>));
 await act(async()=>button('启动')!.click());
 expect(host.querySelector('[role="alert"]')?.textContent).toContain(message);
 expect(button('处理同步冲突')).toBeDefined();expect(open).not.toHaveBeenCalled();expect(refresh).toHaveBeenCalled();
 await act(async()=>button('处理同步冲突')!.click());
 expect(open).toHaveBeenCalledWith(target.id);
});

it('keeps pending conflicts discoverable on the affected profile row before any launch attempt',async()=>{
 const target=(await profiles.listProfiles())[0],open=vi.fn();
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} syncConflicts={[conflict(target.id)]} onResolveSync={open}/>));
 expect(host.textContent).toContain('存在待处理的同步冲突，本机数据已保留。');
 const action=host.querySelector<HTMLButtonElement>(`button[aria-label="处理同步冲突: ${target.name}"]`);
 expect(action).not.toBeNull();await act(async()=>action!.click());expect(open).toHaveBeenCalledWith(target.id);
});

it('opens the sync page at the requested conflict and explains the deletion choice',async()=>{
 const cloud=controller([conflict('one'),conflict('two')]);
 await act(async()=>root.render(<AccountDialog inline cloud={cloud} focusProfileId="two"/>));
 const target=host.querySelector<HTMLElement>('[data-sync-profile="two"]');
 expect(document.activeElement).toBe(target);
 expect(target?.textContent).toContain('选择已删除的一方会同时删除另一方');
 expect(target?.textContent).not.toContain('保留本地会恢复云端副本');
});

it('refreshes conflicts and retains the failed profile when a batch launch is blocked',async()=>{
 const target=(await profiles.listProfiles())[0],open=vi.fn(),refresh=vi.fn();
 vi.spyOn(profiles,'startProfile').mockRejectedValue(new Error('云端配置已变化，请先同步副本配置后再打开浏览器'));
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} onResolveSync={open} onSyncStatusChange={refresh}/>));
 await act(async()=>host.querySelector<HTMLInputElement>(`[aria-label="选择副本 ${target.name}"]`)!.click());
 await act(async()=>button('批量启动')!.click());
 expect(refresh).toHaveBeenCalled();
 await act(async()=>button('处理同步冲突')!.click());expect(open).toHaveBeenCalledWith(target.id);
});

it('retains the newly saved profile target when create-and-start is blocked',async()=>{
 const target=(await profiles.listProfiles())[0],open=vi.fn();
 const start=vi.spyOn(profiles,'startProfile').mockRejectedValue(new Error('云端配置已变化，请先同步副本配置后再打开浏览器'));
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} onResolveSync={open}/>));
 await act(async()=>host.querySelector<HTMLButtonElement>(`[aria-label="${target.name} 更多操作"]`)!.click());
 await act(async()=>button('复制配置')!.click());await act(async()=>button('创建并启动')!.click());
 expect(start).toHaveBeenCalledOnce();
 expect(Array.from(host.querySelectorAll('dialog')).some(dialog=>dialog.open)).toBe(false);
 await act(async()=>button('处理同步冲突')!.click());expect(open).toHaveBeenCalledWith(start.mock.calls[0][0]);
});

it('dismisses restart confirmation and exposes recovery for that profile on failure',async()=>{
 const target={...(await profiles.listProfiles())[0],running:true},open=vi.fn();
 vi.spyOn(profiles,'listProfiles').mockResolvedValue([target]);
 vi.spyOn(profiles,'restartProfile').mockRejectedValue(new Error('云端配置已变化，请先同步副本配置后再打开浏览器'));
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} onResolveSync={open}/>));
 await act(async()=>host.querySelector<HTMLButtonElement>(`[aria-label="${target.name} 更多操作"]`)!.click());
 await act(async()=>button('重启副本')!.click());
 const confirm=Array.from(host.querySelectorAll<HTMLButtonElement>('dialog button')).find(b=>b.textContent?.trim()==='保存后重启')!;
 await act(async()=>confirm.click());
 expect(Array.from(host.querySelectorAll('dialog')).some(dialog=>dialog.open)).toBe(false);
 await act(async()=>button('处理同步冲突')!.click());expect(open).toHaveBeenCalledWith(target.id);
});

it('links the sync settings conflict status directly to resolution',async()=>{
 const cloud=controller([conflict('one')]);
 await act(async()=>root.render(<CloudSettings cloud={cloud}/>));
 expect(button('处理同步冲突')).toBeDefined();await act(async()=>button('处理同步冲突')!.click());expect(cloud.open).toHaveBeenCalledWith('sync');
});

it.each([['local','保留本地'],['cloud','使用云端'],['both','保留两份']])('completes the visible recovery flow for %s and removes resolved conflict indicators',async(choice,label)=>{
 const target=(await profiles.listProfiles())[0],actions:unknown[]=[];
 function Harness(){
  const [pending,setPending]=useState([conflict(target.id)]),[panel,setPanel]=useState<Controller['panel']>(null);
  const cloud:Controller={...controller(pending),panel,setPanel,act:async <T,>(action:Parameters<Controller['act']>[0])=>{actions.push(action);if(action.type==='resolve')setPending([]);return (action.type==='preview'?{profiles:[],localProfiles:[]}: {}) as T;}};
  return <><BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} syncConflicts={pending} onResolveSync={()=>setPanel('sync')}/>{panel==='sync'&&<AccountDialog inline cloud={cloud} focusProfileId={target.id}/>}</>;
 }
 await act(async()=>root.render(<Harness/>));
 await act(async()=>host.querySelector<HTMLButtonElement>(`[aria-label="处理同步冲突: ${target.name}"]`)!.click());
 expect(button('应用选择')).toBeUndefined();
 expect(actions.some(action=>(action as {type:string}).type==='resolve')).toBe(false);
 await act(async()=>button(label)!.click());
 expect(actions.some(action=>(action as {type:string}).type==='resolve')).toBe(false);
 await act(async()=>button(`确认${label}`)!.click());
 expect(actions).toContainEqual({type:'resolve',id:target.id,kind:'config',choice});
 expect(host.querySelector('[data-sync-profile]')).toBeNull();
 expect(host.textContent).not.toContain('存在待处理的同步冲突，本机数据已保留。');
});
