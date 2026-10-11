// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import BrowserSiteShortcuts from './BrowserSiteShortcuts';
import {setLanguage} from './i18n';
const state=vi.hoisted(()=>({catalog:{revision:4,groups:[{id:'research',name:'研究',nameEn:'Research',icon:'ai',order:0,enabled:true,sites:[{id:'chat',name:'ChatGPT',url:'https://chatgpt.com',icon:'chatgpt',order:0,enabled:true},{id:'claude',name:'Claude',url:'https://claude.ai',icon:'claude',order:1,enabled:true}]}]},ready:true}));
vi.mock('./browser-site-catalog',async original=>({...await original<object>(),useBrowserSiteCatalog:()=>state}));
let host:HTMLDivElement,root:Root;
beforeEach(()=>{setLanguage('en');host=document.createElement('div');document.body.append(host);root=createRoot(host);Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();state.ready=true;vi.clearAllMocks();});
it('renders a published custom group, distinct brand icons and opens only a revision-bound site ID',async()=>{
 const open=vi.fn();await act(async()=>root.render(<BrowserSiteShortcuts disabled={false} onOpen={open}/>));
 expect(host.textContent).toBe('Research');
 await act(async()=>host.querySelector('button')!.click());
 const icons=Array.from(host.querySelectorAll('img')).map(i=>i.src);
 expect(icons).toHaveLength(2);expect(new Set(icons).size).toBe(2);
 const buttons=host.querySelectorAll<HTMLButtonElement>('[role=menuitem]');
 await act(async()=>buttons[1].click());
 expect(open).toHaveBeenCalledWith('claude',4);expect(host.querySelector('[role=menu]')).toBeNull();
});
it('keeps shortcuts unavailable until the launch catalog has loaded',async()=>{
 state.ready=false;await act(async()=>root.render(<BrowserSiteShortcuts disabled={false} onOpen={()=>{}}/>));
 expect(host.querySelector('button')!.disabled).toBe(true);
});
