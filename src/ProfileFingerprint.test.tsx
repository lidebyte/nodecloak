// @vitest-environment jsdom
import {act,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {expect,it,vi} from 'vitest';
import ProfileFingerprint from './ProfileFingerprint';
import {defaults} from './browser-profiles';
vi.mock('./bridge',()=>({native:false}));
vi.mock('./i18n',()=>({t:(s:unknown)=>s}));
it('cancels regeneration without changes and confirms a new seed while retaining noise choices',async()=>{
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};
 const draft=defaults(),seed=draft.preferences.fingerprint!.seed;draft.preferences.fingerprint!.fontNoise=false;
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 function Harness(){const [value,setValue]=useState(draft);return <><ProfileFingerprint draft={value} onChange={setValue}/><output>{JSON.stringify(value.preferences.fingerprint)}</output></>;}
 const click=async(text:string)=>{const button=[...host.querySelectorAll('button')].find(b=>b.textContent?.trim()===text);expect(button).toBeTruthy();await act(async()=>button!.click());};
 try{
  await act(async()=>root.render(<Harness/>));await click('重新生成指纹');await click('取消');
  expect(JSON.parse(host.querySelector('output')!.textContent!).seed).toBe(seed);
  await click('重新生成指纹');await click('确认重新生成');
  const changed=JSON.parse(host.querySelector('output')!.textContent!);
  expect(changed.seed).not.toBe(seed);expect(changed.fontNoise).toBe(false);expect(changed.enabled).toBe(true);
  expect(host.querySelector('dialog')!.open).toBe(false);
 }finally{await act(async()=>root.unmount());host.remove();}
});
