// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {AccountDialog,useCloudAccount} from './AccountWorkspace';
import {setLanguage} from './i18n';
const api=vi.hoisted(()=>({action:vi.fn(),status:vi.fn()}));
vi.mock('./bridge',()=>({native:true}));
vi.mock('@tauri-apps/api/event',()=>({listen:async()=>()=>{}}));
vi.mock('./cloud',async importOriginal=>({...await importOriginal<object>(),cloudAction:api.action,cloudStatus:api.status}));
let root:Root,host:HTMLDivElement;
beforeEach(()=>{vi.useFakeTimers();setLanguage('en');host=document.createElement('div');document.body.append(host);root=createRoot(host);Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.useRealTimers();vi.clearAllMocks();});
it('finishes Google reauthentication after the profile view unmounts',async()=>{
 let status={account:{id:'test',name:'Test',email:'test@example.com',avatar:'',googleLinked:true,hasPassword:false,recentAuth:false}};
 api.status.mockImplementation(async()=>status);
 api.action.mockImplementation(async(action)=>{if(action.type==='googlePoll')status={account:{...status.account,recentAuth:true}};return {status,result:action.type==='googleStart'?{pending:true}:{}};});
 function App({showProfile}:{showProfile:boolean}){const cloud=useCloudAccount();return <><span id="auth-state">{String(cloud.status.account?.recentAuth)}</span>{showProfile&&<AccountDialog inline cloud={{...cloud,panel:'account'}}/>}</>;}
 await act(async()=>root.render(<App showProfile/>));
 const google=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.includes('Verify with Google again'))!;
 expect(google).toBeDefined();
 await act(async()=>google.click());
 expect(api.action.mock.calls.map(c=>c[0].type)).toContain('googleStart');
 await act(async()=>root.render(<App showProfile={false}/>));
 await act(async()=>vi.advanceTimersByTimeAsync(2100));
 expect(host.querySelector('#auth-state')?.textContent).toBe('true');
 expect(api.action.mock.calls.filter(c=>c[0].type==='googlePoll')).toHaveLength(1);
 await act(async()=>vi.advanceTimersByTimeAsync(6000));
 expect(api.action.mock.calls.filter(c=>c[0].type==='googlePoll')).toHaveLength(1);
});
it('opts out without initializing a vault, requesting recovery or transferring profiles',async()=>{
 const status={account:{id:'test',name:'Test',email:'test@example.com',avatar:''},unlocked:false,setupComplete:false};
 api.status.mockResolvedValue(status);
 api.action.mockResolvedValue({status:{...status,setupComplete:true,autoSync:false,sessionSync:false,pullCloud:false},result:{}});
 function App(){const cloud=useCloudAccount();return <AccountDialog inline cloud={{...cloud,panel:'sync'}}/>;}
 await act(async()=>root.render(<App/>));
 const skip=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.includes('Do not sync yet'))!;
 expect(skip).toBeDefined();
 await act(async()=>skip.click());
 expect(api.action.mock.calls.map(c=>c[0])).toEqual([{type:'setup',mode:'none',localIds:[],cloudIds:[],sessions:false}]);
});
