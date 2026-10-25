// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import * as profiles from './browser-profiles';
import {setLanguage} from './i18n';
import ProfileReset from './ProfileReset';
vi.mock('./bridge',()=>({native:false}));
let host:HTMLDivElement,root:Root;
beforeEach(()=>{localStorage.clear();setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.restoreAllMocks();});
const button=(text:string)=>Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()===text);
async function openReset(){const p=(await profiles.listProfiles()).find(p=>!p.deletedAt)!;await act(async()=>host.querySelector<HTMLButtonElement>(`[aria-label="${p.name} 更多操作"]`)!.click());expect(button('一键重置')).toBeDefined();await act(async()=>button('一键重置')!.click());return p;}
it('requires confirmation, defaults to keeping proxy, and cancel performs no reset',async()=>{
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
 const p=await openReset();
 const dialog=host.querySelector('dialog[open]')!;
 expect(dialog.textContent).toContain('重置这个环境？');expect(dialog.textContent).toContain(p.name);expect(dialog.textContent).toContain('登录状态');
 expect(dialog.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked).toBe(false);
 expect(button('确认重置')).toBeDefined();
 await act(async()=>button('取消')!.click());
 expect(host.querySelector('dialog[open]')).toBeNull();
 expect((await profiles.listProfiles()).find(v=>v.id===p.id)?.preferences.fingerprint?.seed).toBe(p.preferences.fingerprint?.seed);
});
it('submits once with the proxy checkbox only when explicitly selected',async()=>{
 const p=(await profiles.listProfiles())[0];let finish!:()=>void;
 const reset=vi.spyOn(profiles,'resetProfile').mockImplementation(()=>new Promise<void>(resolve=>{finish=resolve;}));
 const complete=vi.fn(),close=vi.fn(),busy=vi.fn();
 await act(async()=>root.render(<ProfileReset profile={p} busy={false} onBusyChange={busy} onClose={close} onComplete={complete}/>));
 expect(document.activeElement?.textContent).toBe('取消');
 await act(async()=>host.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click());
 await act(async()=>{button('确认重置')!.click();button('确认重置')!.click();});
 expect(reset).toHaveBeenCalledTimes(1);expect(reset).toHaveBeenCalledWith(p.id,true);expect(button('取消')?.disabled).toBe(true);
 expect(host.textContent).toContain('正在重置环境');
 await act(async()=>finish());expect(complete).toHaveBeenCalledOnce();expect(busy.mock.calls).toEqual([[true],[false]]);
});
it('keeps confirmation open on failure and retries without resetting the proxy by default',async()=>{
 const p=(await profiles.listProfiles())[0];const complete=vi.fn();
 const reset=vi.spyOn(profiles,'resetProfile').mockRejectedValueOnce(new Error('请先关闭副本再重置环境')).mockResolvedValue(undefined);
 await act(async()=>root.render(<ProfileReset profile={p} busy={false} onBusyChange={()=>{}} onClose={()=>{}} onComplete={complete}/>));
 await act(async()=>button('确认重置')!.click());
 expect(host.querySelector('[role="alert"]')?.textContent).toBe('请先关闭副本再重置环境');expect(complete).not.toHaveBeenCalled();
 expect(button('确认重置')?.disabled).toBe(false);expect(host.querySelector<HTMLInputElement>('input')?.checked).toBe(false);
 await act(async()=>button('确认重置')!.click());expect(reset.mock.calls).toEqual([[p.id,false],[p.id,false]]);expect(complete).toHaveBeenCalledOnce();
});
it('clears a previously selected proxy option when the dialog is reopened',async()=>{
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
 await openReset();await act(async()=>host.querySelector<HTMLInputElement>('dialog[open] input[type="checkbox"]')!.click());
 await act(async()=>host.querySelector('dialog[open]')!.dispatchEvent(new Event('cancel',{bubbles:true,cancelable:true})));expect(host.querySelector('dialog[open]')).toBeNull();await openReset();
 expect(host.querySelector<HTMLInputElement>('dialog[open] input[type="checkbox"]')?.checked).toBe(false);
});
it('disables reset for running profiles and hides it for received shares',async()=>{
 const p=(await profiles.listProfiles())[0];
 const list=vi.spyOn(profiles,'listProfiles').mockResolvedValue([{...p,running:true}]);
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
 await act(async()=>host.querySelector<HTMLButtonElement>(`[aria-label="${p.name} 更多操作"]`)!.click());
 expect(button('一键重置')?.disabled).toBe(true);expect(button('一键重置')?.title).toContain('请先关闭');
 list.mockResolvedValue([{...p,running:false,sharing:{status:'accepted'} as never}]);
 await act(async()=>button('刷新副本列表')?.click());
 await act(async()=>root.render(<BrowserProfiles key="shared" busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
 await act(async()=>host.querySelector<HTMLButtonElement>(`[aria-label="${p.name} 更多操作"]`)!.click());
 expect(button('一键重置')).toBeUndefined();
});
