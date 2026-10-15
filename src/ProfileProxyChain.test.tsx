// @vitest-environment jsdom
import {act,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {expect,it,vi} from 'vitest';
import ProfileProxyChain from './ProfileProxyChain';
import {setLanguage} from './i18n';
import type {ProxyConfig} from './browser-profiles';

it('configures an optional local first hop with branded controls and a route preview',async()=>{
 vi.stubGlobal('ResizeObserver',class {observe(){} disconnect(){}});HTMLElement.prototype.scrollIntoView=()=>{};
 setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 let saved:ProxyConfig={mode:'socks5',host:'remote.invalid',port:1080,username:'account'};
 function Harness(){const [proxy,set]=useState(saved);return <ProfileProxyChain proxy={proxy} onChange={p=>{saved=p;set(p);}}/>;}
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 try{
  await act(async()=>root.render(<Harness/>));
  const toggle=host.querySelector<HTMLInputElement>('input[role=switch]')!;expect(toggle).not.toBeNull();expect(toggle.checked).toBe(false);
  await act(async()=>toggle.click());expect(saved.preProxy).toEqual({mode:'http',host:'127.0.0.1',port:7890});
  expect(host.querySelector('[aria-label="代理链路"]')?.textContent).toContain('前置代理');
  expect(host.querySelector('[aria-label="代理链路"]')?.textContent).toContain('remote.invalid:1080');
  await act(async()=>host.querySelector<HTMLButtonElement>('[role=combobox]')!.click());
  const socks=[...document.querySelectorAll<HTMLButtonElement>('[role=option]')].find(b=>b.textContent?.includes('SOCKS5'))!;
  await act(async()=>socks.click());expect(saved.preProxy?.mode).toBe('socks5');
  const port=host.querySelector<HTMLInputElement>('[aria-label="前置代理端口"]')!;
  await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(port,'1081');port.dispatchEvent(new Event('input',{bubbles:true}));});
  expect(saved.preProxy?.port).toBe(1081);
  const address=host.querySelector<HTMLInputElement>('[aria-label="前置代理地址"]')!;
  await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(address,'192.168.10.1');address.dispatchEvent(new Event('input',{bubbles:true}));});
  expect(saved.preProxy?.host).toBe('192.168.10.1');
  expect(host.querySelector('[aria-label="代理链路"]')?.textContent).toContain('192.168.10.1:1081');
  await act(async()=>toggle.click());expect(saved.preProxy).toBeUndefined();
  expect(host.querySelector('[aria-label="前置代理端口"]')).toBeNull();
 }finally{await act(async()=>root.unmount());host.remove();vi.unstubAllGlobals();}
});
