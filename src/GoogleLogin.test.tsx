// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {AccountDialog,useCloudAccount,type Controller} from './AccountWorkspace';
import {setLanguage} from './i18n';
import type {CloudStatus} from './cloud';

const api=vi.hoisted(()=>({action:vi.fn(),status:vi.fn()}));
vi.mock('./bridge',()=>({native:true}));
vi.mock('@tauri-apps/api/event',()=>({listen:async()=>()=>{}}));
vi.mock('./cloud',async importOriginal=>({...await importOriginal<object>(),cloudAction:api.action,cloudStatus:api.status}));
let root:Root,host:HTMLDivElement,cloud:Controller;
const signedOut:CloudStatus={account:null};
function deferred<T>(){
 let resolve!:(value:T)=>void,reject!:(reason:Error)=>void;
 const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});
 return {promise,resolve,reject};
}
function App(){cloud=useCloudAccount();return <AccountDialog inline cloud={{...cloud,panel:'account'}}/>;}
function google(){return host.querySelector<HTMLButtonElement>('.google-login')!;}
function emailSubmit(){return host.querySelector<HTMLButtonElement>('.account-login button[type="submit"]')!;}
beforeEach(()=>{
 vi.useFakeTimers();setLanguage('en');api.status.mockResolvedValue(signedOut);
 host=document.createElement('div');document.body.append(host);root=createRoot(host);
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.useRealTimers();vi.resetAllMocks();});

it('keeps Google loading on its button from startup through repeated background checks',async()=>{
 const start=deferred<{status:CloudStatus,result:{pending:boolean}}>();
 let poll=deferred<{status:CloudStatus,result:{pending:boolean}}>();
 api.action.mockImplementation(action=>action.type==='googleStart'?start.promise:poll.promise);
 await act(async()=>root.render(<App/>));
 await act(async()=>google().click());
 expect(google().querySelector('.spin')).not.toBeNull();
 expect(google().getAttribute('aria-busy')).toBe('true');
 expect(host.querySelector('.account-progress')).toBeNull();
 expect(emailSubmit().querySelector('.spin')).toBeNull();
 expect(emailSubmit().disabled).toBe(true);
 expect(host.querySelector<HTMLInputElement>('input[type="email"]')?.disabled).toBe(true);
 await act(async()=>start.resolve({status:signedOut,result:{pending:true}}));
 for(let check=0;check<3;check++){
  await act(async()=>vi.advanceTimersByTimeAsync(2000));
  expect(cloud.busy).toBe(false);
  expect(google().getAttribute('aria-busy')).toBe('true');
  expect(google().querySelector('.spin')).not.toBeNull();
  expect(host.querySelector('.account-progress')).toBeNull();
  expect(emailSubmit().querySelector('.spin')).toBeNull();
  expect(emailSubmit().disabled).toBe(true);
  expect(host.querySelector<HTMLInputElement>('input[type="email"]')?.disabled).toBe(true);
  await act(async()=>poll.resolve({status:signedOut,result:{pending:true}}));
  poll=deferred();
 }
});

it('ends Google loading when sign-in completes and stops checking',async()=>{
 const finish=deferred<{status:CloudStatus,result:object}>();
 const signedIn:CloudStatus={account:{id:'test',name:'Test',email:'test@example.com',avatar:''}};
 api.action.mockImplementation(action=>action.type==='googleStart'?Promise.resolve({status:signedOut,result:{pending:true}}):finish.promise);
 await act(async()=>root.render(<App/>));
 await act(async()=>google().click());
 await act(async()=>vi.advanceTimersByTimeAsync(2000));
 await act(async()=>finish.resolve({status:signedIn,result:{}}));
 expect(cloud.googlePending).toBe(false);expect(cloud.busy).toBe(false);
 expect(cloud.status.account?.email).toBe('test@example.com');
 expect(host.querySelector('.google-login')).toBeNull();
 expect(host.querySelector('.account-progress')).toBeNull();
 const calls=api.action.mock.calls.length;
 await act(async()=>vi.advanceTimersByTimeAsync(6000));
 expect(api.action.mock.calls).toHaveLength(calls);
});

it.each(['googleStart','googlePoll'])('clears Google loading and permits retry after %s fails',async phase=>{
 api.action.mockImplementation(async action=>{
  if(action.type===phase)throw new Error('Sign-in could not be completed');
  return {status:signedOut,result:{pending:true}};
 });
 await act(async()=>root.render(<App/>));
 await act(async()=>google().click());
 if(phase==='googlePoll')await act(async()=>vi.advanceTimersByTimeAsync(2000));
 expect(google().getAttribute('aria-busy')).toBe('false');
 expect(google().querySelector('.spin')).toBeNull();
 expect(google().disabled).toBe(false);expect(emailSubmit().disabled).toBe(false);
 expect(host.querySelector('[role="alert"]')?.textContent).toContain('Sign-in could not be completed');
 const calls=api.action.mock.calls.length;
 await act(async()=>vi.advanceTimersByTimeAsync(6000));
 expect(api.action.mock.calls).toHaveLength(calls);
});

it('keeps email action loading on the email submit button',async()=>{
 const email=deferred<{status:CloudStatus,result:object}>();
 api.action.mockReturnValue(email.promise);
 await act(async()=>root.render(<App/>));
 let work!:Promise<unknown>;
 await act(async()=>{work=cloud.act({type:'requestEmail',email:'test@example.com'});});
 expect(emailSubmit().querySelector('.spin')).not.toBeNull();
 expect(google().querySelector('.spin')).toBeNull();
 expect(google().getAttribute('aria-busy')).toBe('false');
 await act(async()=>{email.resolve({status:signedOut,result:{}});await work;});
 expect(emailSubmit().querySelector('.spin')).toBeNull();
});
