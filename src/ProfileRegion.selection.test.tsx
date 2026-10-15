// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import {defaults,type BrowserProfile} from './browser-profiles';
import {setLanguage} from './i18n';
const api=vi.hoisted(()=>({list:vi.fn(),save:vi.fn()}));
vi.mock('./bridge',()=>({native:false}));
vi.mock('./browser-profiles',async original=>({...await original<object>(),listProfiles:api.list,saveProfile:api.save}));
let host:HTMLDivElement,root:Root;
const click=async(text:string)=>{const button=[...host.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text);expect(button).toBeTruthy();await act(async()=>button!.click());};
const choose=async(label:string,value:string)=>{
 await act(async()=>host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!.click());
 const search=host.querySelector<HTMLInputElement>('.select-menu-search input');
 if(search)await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(search,value);search.dispatchEvent(new Event('input',{bubbles:true}));});
 const option=[...host.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(b=>b.textContent?.includes(value));expect(option).toBeTruthy();await act(async()=>option!.click());
};
beforeEach(()=>{vi.stubGlobal('ResizeObserver',class{observe(){} disconnect(){} unobserve(){}});HTMLElement.prototype.scrollIntoView=vi.fn();HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.resetAllMocks();vi.unstubAllGlobals();});
for(const browser of ['chrome','edge','firefox'] as const)for(const editing of [false,true])it(`${browser}: saves region dropdown values when ${editing?'editing':'creating'} a profile`,async()=>{
 const profile:BrowserProfile={...defaults(),id:'existing-'+browser,legacy:false,name:'Existing',browser,createdAt:'',updatedAt:'',deletedAt:null,lastTest:null,running:false,pendingRestart:false,browserAvailable:true,hasPassword:false,proxyReady:true};
 profile.preferences.language='en-CA,en';
 api.list.mockResolvedValue(editing?[profile]:[]);api.save.mockResolvedValue(profile);
 await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
 if(editing){await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="Existing 更多操作"]')!.click());await click('编辑设置');}
 else {await click('新建副本');const input=host.querySelector<HTMLInputElement>('input[placeholder="例如：日常使用 / 工作环境"]')!;await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(input,'New profile');input.dispatchEvent(new Event('input',{bubbles:true}));});await choose('浏览器',{chrome:'Chrome',edge:'Edge',firefox:'Firefox'}[browser]);}
 await click('高级设置');
 if(editing)expect(host.querySelector('[aria-label="浏览器语言"]')!.textContent).toContain('en-CA,en');
 const language=editing?'fr-CA,fr,en':'zh-CN,zh';await choose('浏览器语言',language);
 const group=host.querySelector('[role="group"][aria-label="时区"]')!;await act(async()=>[...group.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent==='自定义')!.click());
 await choose('自定义时区','Europe/Paris');await click('保存');
 expect(api.save).toHaveBeenCalledOnce();const [draft,id]=api.save.mock.calls[0];
 expect(draft.browser).toBe(browser);expect(draft.preferences.language).toBe(language);expect(draft.preferences.regional).toMatchObject({enabled:true,timezoneMode:'custom',timezone:'Europe/Paris'});
 expect(id).toBe(editing?profile.id:undefined);expect(draft.preferences.startupUrl).toBe(profile.preferences.startupUrl);
 if(editing)expect(draft.preferences.fingerprint).toEqual(profile.preferences.fingerprint);
});
