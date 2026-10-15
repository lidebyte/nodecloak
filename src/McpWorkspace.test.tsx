// @vitest-environment jsdom
import {it,expect,vi,beforeEach} from 'vitest';
import {createRoot} from 'react-dom/client';
import {act} from 'react';
import McpWorkspace from './McpWorkspace';
import McpRemote from './McpRemote';
import {setLanguage} from './i18n';
beforeEach(()=>{
 HTMLDialogElement.prototype.close=function(){this.removeAttribute('open');};
 HTMLDialogElement.prototype.showModal=function(){this.setAttribute('open','');};
});
it('lets users change confirmation on an existing client without recreating it',async()=>{
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});setLanguage('zh');
 const grant={id:'g',name:'Codex',profileIds:['one'],permissions:['read','page_write'],origins:[],expiresAt:9999999999,confirmInteractions:true,fileRoots:[],revoked:false};
 const status={enabled:true,ready:true,error:null,grants:[grant],jobs:[]};const onAction=vi.fn(async(action:Record<string,unknown>)=>({...status,grants:[{...grant,confirmInteractions:action.kind==='grant_confirmation'?false:true,permissions:action.kind==='grant_sync_permission'?['read','page_write','sync']:grant.permissions}]}));
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 await act(async()=>root.render(<McpWorkspace initialStatus={status} profiles={[]} onAction={onAction}/>));
 const toggle=host.querySelector<HTMLInputElement>('input[aria-label="Codex · 网页操作前确认"]');expect(toggle).not.toBeNull();
 const sync=host.querySelector<HTMLInputElement>('input[aria-label="Codex · 同步副本"]');expect(sync).not.toBeNull();
 await act(async()=>sync!.click());expect(onAction).toHaveBeenCalledWith({kind:'grant_sync_permission',id:'g',enabled:true});
 await act(async()=>toggle!.click());expect(onAction).toHaveBeenCalledWith({kind:'grant_confirmation',id:'g',confirm:false});
 await act(async()=>root.unmount());host.remove();
});
it('shows connected only while a live grant has an active device route',async()=>{
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});setLanguage('zh');
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 const grant={id:'g',name:'Remote AI',profileIds:['one'],permissions:['read'],origins:[],expiresAt:Date.now()/1000+60,confirmInteractions:false,fileRoots:[],revoked:false};
 const remote={routes:[{routeId:'r',localGrantId:'g',endpoint:'https://nodecloak.com',accountId:'a'}],lastConnected:Date.now()/1000,error:null,resource:'https://nodecloak.com/mcp',activeRoutes:['r']};
 const render=async(activeRoutes:string[],expiresAt=grant.expiresAt)=>act(async()=>root.render(<McpRemote status={{enabled:true,ready:true,error:null,grants:[{...grant,expiresAt}],jobs:[],remote:{...remote,activeRoutes}}} onPrepare={()=>{}} onRevoke={()=>{}}/>));
 await render(['r']);expect(host.querySelector('.mcp-status')?.textContent).toBe('连接正常');
 await render([]);expect(host.querySelector('.mcp-status')?.textContent).toBe('等待连接');
 await render(['r'],Date.now()/1000-1);expect(host.querySelector('.mcp-status')?.textContent).toBe('等待连接');expect(host.querySelector('.mcp-route')).toBeNull();
 await act(async()=>root.unmount());host.remove();
});
it('exposes remote pairing in its own tab and keeps approval controls separate',async()=>{
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});setLanguage('zh');
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 await act(async()=>root.render(<McpWorkspace initialStatus={{enabled:true,ready:true,error:null,grants:[],jobs:[]}} profiles={[]}/>));
 const remote=Array.from(host.querySelectorAll('button')).find(b=>b.textContent==='远程连接');expect(remote).toBeDefined();
 await act(async()=>remote!.click());expect(host.textContent).toContain('授权码');expect(host.textContent).toContain('网页内容和截图');
 const input=host.querySelector<HTMLInputElement>('input[aria-label="授权码"]');expect(input?.maxLength).toBe(16);
 await act(async()=>root.unmount());host.remove();
});
it('keeps grants scoped to selected profiles and exposes revoke and approval actions',async()=>{
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 HTMLDialogElement.prototype.close=function(){this.removeAttribute('open');};
 HTMLDialogElement.prototype.showModal=function(){this.setAttribute('open','');};
 setLanguage('zh');const onAction=vi.fn(async()=>({enabled:true,ready:true,error:null,grants:[],jobs:[]}));
 const host=document.createElement('div');document.body.append(host);const root=createRoot(host);
 await act(async()=>root.render(<McpWorkspace initialStatus={{enabled:true,ready:true,error:null,grants:[{id:'g',name:'Research',profileIds:['one'],permissions:['read'],origins:[],expiresAt:9999999999,confirmInteractions:true,fileRoots:[],revoked:false}],jobs:[{id:'j',clientId:'g',tool:'profiles_trash',arguments:{profile_id:'one',value:'private-form-value'},target:{title:'Trusted fixture',url:'https://example.test',name:'Display name'},state:'awaiting_approval',result:null,error:null,createdAt:1}]}} profiles={[{id:'one',name:'Work',browser:'chrome'}]} onAction={onAction}/>));
 expect(host.textContent).toContain('Research');
 await act(async()=>Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.includes('待确认操作'))!.click());
 expect(host.textContent).toContain('Trusted fixture');expect(host.querySelector('.mcp-job > pre')?.textContent).not.toContain('private-form-value');expect(host.querySelector('details')?.open).toBe(false);
 const buttons=Array.from(host.querySelectorAll('button'));await act(async()=>buttons.find(b=>b.textContent?.includes('允许执行'))!.click());
 expect(onAction).toHaveBeenCalledWith({kind:'approve',id:'j'});
 await act(async()=>root.unmount());host.remove();
});
