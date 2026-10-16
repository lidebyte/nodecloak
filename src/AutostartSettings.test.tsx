// @vitest-environment jsdom
// The preference must reflect the OS result and retain an explicit opt-out.
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import App from './App';
import {LanguageProvider} from './LanguageControl';
import {languageStorageKey} from './i18n';
const api=vi.hoisted(()=>({enabled:true,read:vi.fn(),write:vi.fn()}));
vi.mock('./bridge',async original=>({...await original<object>(),getAutostartEnabled:api.read,setAutostartEnabled:api.write}));
vi.mock('./browser-site-catalog',async original=>({...await original<object>(),useBrowserSiteCatalog:()=>({catalog:{revision:0,groups:[],announcements:[]}})}));
vi.mock('./AppUpdates',()=>({default:()=>null}));
let host:HTMLDivElement,root:Root;
beforeEach(()=>{localStorage.clear();localStorage.setItem(languageStorageKey,'zh');api.enabled=true;api.read.mockImplementation(async()=>api.enabled);api.write.mockImplementation(async(value:boolean)=>{api.enabled=value;return value;});Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});vi.stubGlobal('matchMedia',()=>({matches:false,addEventListener:()=>{},removeEventListener:()=>{}}));HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.clearAllMocks();vi.unstubAllGlobals();});
async function preferences(){await act(async()=>host.querySelector<HTMLButtonElement>('.sidebar-account')!.click());const button=Array.from(host.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).find(b=>b.textContent==='偏好设置')!;await act(async()=>button.click());}
const toggle=()=>host.querySelector<HTMLInputElement>('input[role="switch"][aria-label="开机自动启动"]');
it('shows enabled by default and retains a disabled preference after reopening',async()=>{
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await preferences();expect(toggle()).not.toBeNull();expect(toggle()!.checked).toBe(true);
 await act(async()=>toggle()!.click());expect(toggle()!.checked).toBe(false);
 await act(async()=>root.unmount());root=createRoot(host);await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await preferences();expect(toggle()!.checked).toBe(false);
});
it('reports a failed OS change without showing the requested state as saved',async()=>{
 await act(async()=>root.render(<LanguageProvider><App/></LanguageProvider>));await preferences();expect(toggle()).not.toBeNull();
 api.write.mockRejectedValueOnce(new Error('Startup registration denied'));await act(async()=>toggle()!.click());
 expect(toggle()!.checked).toBe(true);expect(host.querySelector('[role="alert"]')?.textContent).toContain('Startup registration denied');expect(toggle()!.disabled).toBe(false);
});
