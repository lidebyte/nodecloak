// @vitest-environment jsdom
import {act} from 'react';
import {createRoot} from 'react-dom/client';
import {expect,it,vi} from 'vitest';
import BrowserProfiles from './BrowserProfiles';
import {defaults,type BrowserProfile} from './browser-profiles';
import {setLanguage} from './i18n';
const api=vi.hoisted(()=>({list:vi.fn(),save:vi.fn(),test:vi.fn()}));
vi.mock('./bridge',()=>({native:false}));
vi.mock('./browser-profiles',async original=>({...await original<object>(),listProfiles:api.list,saveProfile:api.save,testProxy:api.test}));
it('reopens API settings and passes the edited source and local chain to tests and save, surfacing provider errors',async()=>{
 setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};
 const profile:BrowserProfile={...defaults(),id:'api-fixture',name:'API fixture',browser:'chrome',createdAt:'',updatedAt:'',deletedAt:null,lastTest:null,legacy:false,running:false,pendingRestart:false,browserAvailable:true,hasPassword:false,proxyReady:true};
 profile.proxy={mode:'http',host:'',port:0,username:'',apiUrl:'https://provider.invalid/old',preProxy:{mode:'socks5',host:'127.0.0.1',port:1080}};
 api.list.mockResolvedValue([profile]);api.save.mockResolvedValue(profile);api.test.mockRejectedValue(new Error('代理 API 返回异常（应为 IP:端口）：quota exceeded'));
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 const click=async(text:string)=>{const button=[...host.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text);expect(button).toBeTruthy();await act(async()=>button!.click());};
 try{
  await act(async()=>root.render(<BrowserProfiles busy={false} onBusyChange={()=>{}} onSelect={()=>{}}/>));
  await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="API fixture 更多操作"]')!.click());await click('编辑设置');await click('代理设置');
  expect(host.querySelector('[aria-label="粘贴代理链接"]')).toBeNull();
  const field=host.querySelector<HTMLInputElement>('[aria-label="代理 API 地址"]')!;expect(field.value).toBe('https://provider.invalid/old');
  await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(field,'https://provider.invalid/new');field.dispatchEvent(new Event('input',{bubbles:true}));});
  await click('测试连接');expect(api.test).toHaveBeenCalledOnce();expect(api.test.mock.calls[0][0].proxy).toMatchObject({apiUrl:'https://provider.invalid/new',preProxy:{port:1080}});
  expect(host.textContent).toContain('quota exceeded');
  await click('保存');expect(api.save).toHaveBeenCalledOnce();expect(api.save.mock.calls[0][0].proxy).toMatchObject({apiUrl:'https://provider.invalid/new',preProxy:{port:1080}});
 }finally{await act(async()=>root.unmount());host.remove();vi.resetAllMocks();}
});
