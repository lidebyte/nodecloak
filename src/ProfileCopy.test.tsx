// @vitest-environment jsdom
import {act} from 'react';
import {createRoot} from 'react-dom/client';
import {expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import {defaults,type BrowserProfile,requiresBrowserControl} from './browser-profiles';
import {setLanguage} from './i18n';
const api=vi.hoisted(()=>({list:vi.fn(),save:vi.fn()}));
vi.mock('./bridge',()=>({native:false}));
vi.mock('./browser-profiles',async original=>({...await original<object>(),listProfiles:api.list,saveProfile:api.save}));
it('copying a default profile preserves native login mode instead of enabling fingerprint automation',async()=>{
 setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};
 const p:BrowserProfile={...defaults(),id:'legacy-chrome',name:'Chrome',browser:'chrome',createdAt:'',updatedAt:'',deletedAt:null,lastTest:null,legacy:true,running:false,pendingRestart:false,browserAvailable:true,hasPassword:false,proxyReady:true};
 delete p.preferences.fingerprint;
 api.list.mockResolvedValue([p]);api.save.mockResolvedValue({...p,id:'new-copy'});
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 const click=async(label:string)=>{const button=[...host.querySelectorAll('button')].find(b=>b.textContent?.trim()===label);expect(button).toBeTruthy();await act(async()=>button!.click());};
 try {
  await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
  await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="Chrome 更多操作"]')!.click());
  await click('复制配置');await click('保存');
  expect(api.save).toHaveBeenCalledOnce();
  const [draft,id,copyId]=api.save.mock.calls[0];
  expect(id).toBeUndefined();expect(copyId).toBe(p.id);
  expect(draft.preferences.fingerprint.seed).toMatch(/^[0-9a-f]{64}$/);
  expect(draft.preferences.fingerprint.enabled).toBe(false);
  expect(requiresBrowserControl('chrome',draft.preferences)).toBe(false);
 } finally {await act(async()=>root.unmount());host.remove();vi.resetAllMocks();}
});
