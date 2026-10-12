// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import {defaults,type BrowserProfile} from './browser-profiles';
import {setLanguage} from './i18n';
const api=vi.hoisted(()=>({list:vi.fn(),start:vi.fn(),listeners:new Map<string,()=>void>()}));vi.mock('./bridge',()=>({native:true}));vi.mock('@tauri-apps/api/event',()=>({listen:async(name:string,callback:()=>void)=>{api.listeners.set(name,callback);return()=>api.listeners.delete(name);}}));vi.mock('./browser-profiles',async original=>({...await original<object>(),listProfiles:api.list,startProfile:api.start}));
let host:HTMLDivElement,root:Root;
const shared=():BrowserProfile=>({...defaults(),id:'restricted',name:'From Jane',createdAt:'',updatedAt:'',deletedAt:null,lastTest:null,legacy:false,running:false,pendingRestart:false,browserAvailable:true,hasPassword:false,proxyReady:true,sharing:{id:'grant',senderName:'Jane',expiresAt:new Date(Date.now()+86400000).toISOString(),status:'accepted'}});
beforeEach(()=>{setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();api.listeners.clear();vi.useRealTimers();vi.resetAllMocks();});
it('keeps received profiles out of edit, copy, re-share and ordinary delete flows',async()=>{const share=vi.fn(),manage=vi.fn();api.list.mockResolvedValue([shared()]);await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}} onShare={share} onManageSharing={manage}/>));expect(host.textContent).toContain('来自 Jane');await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="From Jane 更多操作"]')!.click());const menu=host.querySelector('.profile-menu')!;expect(menu.textContent).not.toContain('编辑设置');expect(menu.textContent).not.toContain('复制配置');expect(menu.textContent).not.toContain('分享给朋友');expect(menu.textContent).not.toContain('删除');const button=Array.from(menu.querySelectorAll('button')).find(b=>b.textContent==='管理共享')!;await act(async()=>button.click());expect(manage).toHaveBeenCalledOnce();expect(share).not.toHaveBeenCalled();});
it('disables launching a locally expired share even while metadata still reports accepted',async()=>{api.list.mockResolvedValue([{...shared(),sharing:{...shared().sharing!,expiresAt:new Date(Date.now()-1000).toISOString()}}]);await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));const launch=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()==='启动')!;expect(launch.disabled).toBe(true);expect(host.textContent).toContain('已过期');expect(api.start).not.toHaveBeenCalled();});

it('keeps launch errors visible through polling and native refresh events until dismissed',async()=>{
  vi.useFakeTimers();
  api.list.mockResolvedValue([{...shared(),sharing:undefined,name:'Local Firefox'}]);
  api.start.mockRejectedValue('此副本包含分区网站存储，暂不支持完整迁移');
  await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
  const launch=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()==='启动')!;
  await act(async()=>launch.click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('分区网站存储');
  await act(async()=>api.listeners.get('profiles-changed')!());
  await act(async()=>api.listeners.get('sharing-changed')!());
  await act(async()=>vi.advanceTimersByTimeAsync(18000));
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('分区网站存储');
  await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="关闭错误提示"]')!.click());
  expect(host.querySelector('[role="alert"]')).toBeNull();
  await act(async()=>vi.advanceTimersByTimeAsync(6000));
  expect(host.querySelector('[role="alert"]')).toBeNull();
});

it('clears recovered list errors and clears an operation error when the user retries successfully',async()=>{
  api.list.mockRejectedValueOnce('副本列表读取失败').mockResolvedValue([{...shared(),sharing:undefined}]);
  await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('副本列表读取失败');
  await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="刷新副本列表"]')!.click());
  expect(host.querySelector('[role="alert"]')).toBeNull();
  api.start.mockRejectedValueOnce('启动失败').mockResolvedValueOnce(undefined);
  const launch=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()==='启动')!;
  await act(async()=>launch.click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('启动失败');
  await act(async()=>launch.click());
  expect(host.querySelector('[role="alert"]')).toBeNull();
});
