// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {useProfileSharing,type SharingController} from './ProfileSharing';
import type {SharingList} from './profile-sharing';
const api=vi.hoisted(()=>({action:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('./bridge',()=>({native:true}));
vi.mock('./profile-sharing',async original=>({...await original<object>(),sharingAction:api.action}));
vi.mock('@tauri-apps/api/event',()=>({listen:async(name:string,fn:(event:{payload:unknown})=>void)=>{api.listeners.set(name,fn);return()=>api.listeners.delete(name);}}));
let root:Root,host:HTMLDivElement,sharing:SharingController;
const list=(name:string):SharingList=>({sent:[],received:[{id:name,sourceProfileId:'p',profileName:name,browser:'firefox',senderId:'s',senderName:name,senderEmail:'s@example.com',recipientId:'r',recipientName:'r',recipientEmail:'r@example.com',includesSession:false,note:'',status:'pending',createdAt:'',expiresAt:'2099-01-01T00:00:00Z',acceptedAt:null}],serverTime:new Date().toISOString()});
function deferred<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(yes=>resolve=yes);return {promise,resolve};}
function App({account}:{account?:string}){sharing=useProfileSharing(account);return <div>{sharing.data.received.map(s=>s.profileName)}{sharing.warning}</div>;}
beforeEach(()=>{vi.useFakeTimers();Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.useRealTimers();api.listeners.clear();vi.resetAllMocks();});
it('discards delayed metadata from a previous account',async()=>{const old=deferred<SharingList>();api.action.mockReturnValueOnce(old.promise).mockResolvedValueOnce(list('New'));await act(async()=>root.render(<App account="old"/>));await act(async()=>root.render(<App account="new"/>));expect(host.textContent).toBe('New');await act(async()=>old.resolve(list('Old')));expect(host.textContent).toBe('New');await act(async()=>root.render(<App/>));expect(host.textContent).toBe('');});
it('preserves the visible list and foreground busy state during background checks',async()=>{const poll=deferred<SharingList>();api.action.mockResolvedValueOnce(list('Friend')).mockReturnValueOnce(poll.promise);await act(async()=>root.render(<App account="me"/>));await act(async()=>vi.advanceTimersByTimeAsync(30000));expect(host.textContent).toBe('Friend');expect(sharing.loading).toBe(false);expect(sharing.busy).toBe(false);await act(async()=>poll.resolve(list('Friend')));expect(host.textContent).toBe('Friend');});
it('shows native expiry warnings while keeping existing invitations visible',async()=>{api.action.mockResolvedValue(list('Friend'));await act(async()=>root.render(<App account="me"/>));await act(async()=>api.listeners.get('sharing-changed')!({payload:{reason:'expiring',id:'s'}}));expect(host.textContent).toContain('5 分钟');expect(host.textContent).toContain('Friend');});
it('keeps actionable send failures visible through background polling and permits retry',async()=>{
 api.action.mockResolvedValue(list('Friend'));await act(async()=>root.render(<App account="me"/>));
 api.action.mockRejectedValueOnce('云端请求失败 (409): recipient_device_approval_required');
 const invitation={type:'create' as const,profileId:'own',name:'Friend',email:'friend@example.com',expiresAt:'2099-01-01T00:00:00Z',includesSession:false,note:''};
 await act(async()=>{expect(await sharing.act(invitation)).toBeNull();});
 expect(sharing.error).toContain('登录 NodeCloak');expect(sharing.error).not.toContain('管理同步');expect(sharing.notice).toBe('');expect(sharing.busy).toBe(false);
 await act(async()=>vi.advanceTimersByTimeAsync(30000));expect(sharing.error).toContain('登录 NodeCloak');
 api.action.mockResolvedValueOnce(list('Friend').received[0]);await act(async()=>{await sharing.act(invitation);});
 expect(sharing.error).toBe('');expect(sharing.notice).toContain('邀请已发送');
});
