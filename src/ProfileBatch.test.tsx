// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import * as profiles from './browser-profiles';
import {setLanguage} from './i18n';
vi.mock('./bridge',()=>({native:false}));
let host:HTMLDivElement,root:Root;
beforeEach(()=>{vi.resetModules();localStorage.clear();setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
const click=async(text:string)=>{const b=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()===text);expect(b, text).toBeTruthy();await act(async()=>b!.click());};
it('bulk-deletes only selected filtered profiles and can restore them from trash',async()=>{
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
 const search=host.querySelector<HTMLInputElement>('[aria-label="搜索浏览器副本"]')!;
 await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(search,'Chrome');search.dispatchEvent(new Event('input',{bubbles:true}));});
 const all=host.querySelector<HTMLInputElement>('[aria-label="选择当前列表"]');expect(all).toBeTruthy();
 await act(async()=>all!.click());await click('批量删除');await click('移至最近删除');
 expect((await profiles.listProfiles()).filter(p=>p.deletedAt).map(p=>p.name)).toEqual(['Chrome']);
 await click('最近删除');await act(async()=>host.querySelector<HTMLInputElement>('[aria-label="选择当前列表"]')!.click());await click('批量恢复');
 expect((await profiles.listProfiles()).every(p=>!p.deletedAt)).toBe(true);
});
