// @vitest-environment jsdom
import {act,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {expect,it,vi} from 'vitest';
import ProfileProxySource from './ProfileProxySource';
import {setLanguage} from './i18n';
import type {ProxyConfig} from './browser-profiles';
it('switches to API extraction, edits URL, and preserves protocol, authentication and local chain',async()=>{
 setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 vi.stubGlobal('ResizeObserver',class {observe(){} disconnect(){}});HTMLElement.prototype.scrollIntoView=()=>{};
 let value:ProxyConfig={mode:'socks5',host:'static.invalid',port:1080,username:'account',preProxy:{mode:'http',host:'127.0.0.1',port:7890}};
 function Harness(){const [proxy,set]=useState(value);return <ProfileProxySource proxy={proxy} onChange={next=>{value=next;set(next);}}/>;}
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 const choose=async(text:string)=>{
  await act(async()=>host.querySelector<HTMLButtonElement>('[role=combobox]')!.click());
  const option=[...document.querySelectorAll<HTMLButtonElement>('[role=option]')].find(b=>b.textContent?.includes(text))!;
  expect(option).toBeTruthy();await act(async()=>option.click());
 };
 try{
  await act(async()=>root.render(<Harness/>));expect(host.querySelector('[role=combobox]')).not.toBeNull();
  await choose('API 提取');expect(value).toMatchObject({mode:'socks5',apiUrl:'',host:'',port:0,username:'account',preProxy:{port:7890}});
  const field=host.querySelector<HTMLInputElement>('[aria-label="代理 API 地址"]')!;
  await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(field,'https://provider.invalid/extract');field.dispatchEvent(new Event('input',{bubbles:true}));});
  expect(value.apiUrl).toBe('https://provider.invalid/extract');
  await choose('手动填写');expect(value.apiUrl).toBeUndefined();expect(value.preProxy?.port).toBe(7890);expect(host.querySelector('[aria-label="代理 API 地址"]')).toBeNull();
 }finally{await act(async()=>root.unmount());host.remove();vi.unstubAllGlobals();}
});
