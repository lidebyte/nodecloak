// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import SyncWorkspace from './SyncWorkspace';
import type {Controller} from './AccountWorkspace';
import type {CloudConflict,CloudStatus} from './cloud';
import {setLanguage} from './i18n';

const account={id:'test',name:'Tester',email:'test@example.invalid',avatar:''};
const issue=(kind:CloudConflict['kind']='config',fields=['/name']):CloudConflict=>({localId:'one',cloudId:'remote',name:'My browser',kind,fields,remoteRevision:2});
let host:HTMLDivElement,root:Root,cloud:Controller;
beforeEach(()=>{setLanguage('zh');Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});host=document.createElement('div');document.body.append(host);root=createRoot(host);
 const status:CloudStatus={account,unlocked:true,recoverySaved:true,setupComplete:true,autoSync:true,sessionSync:true,conflicts:[issue()],bindings:[{localId:'one',cloudId:'remote',configPending:false,sessionPending:false}]};
 cloud={status,panel:'sync',busy:false,error:'',googlePending:false,beginGoogle:async()=>null,setError:vi.fn(),setPanel:vi.fn(),open:vi.fn(),reload:vi.fn(),act:vi.fn(async action=>action.type==='preview'?{localProfiles:[{id:'one',name:'My browser',browser:'chrome',eligible:true,selected:true}],profiles:[]} as never:{} as never)};
});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.restoreAllMocks();});
async function render(){await act(async()=>root.render(<SyncWorkspace cloud={cloud}/>));}
async function click(label:string){const b=Array.from(host.querySelectorAll('button')).find(b=>b.textContent?.trim()===label);expect(b,`button: ${label}`).toBeDefined();await act(async()=>b!.click());}
it('puts attention before profile management and confirms the concrete effect before resolving',async()=>{
 await render();expect(host.querySelector('select')).toBeNull();expect(host.textContent).not.toContain('应用选择');
 expect(host.innerHTML.indexOf('需要你处理')).toBeLessThan(host.innerHTML.indexOf('已关联副本'));
 await click('保留本地');expect(cloud.act).not.toHaveBeenCalledWith(expect.objectContaining({type:'resolve'}));
 expect(host.querySelector('[role="alert"]')?.textContent).toContain('覆盖云端配置');
 await click('取消');expect(host.querySelector('.sync-confirm')).toBeNull();
 await click('保留本地');await click('确认保留本地');
 expect(cloud.act).toHaveBeenCalledWith({type:'resolve',id:'one',kind:'config',choice:'local'});
 expect(host.querySelector('[role="status"]')?.textContent).toContain('已处理');
});
it('requires configuration resolution before offering login-state actions',async()=>{
 cloud.status.conflicts=[issue(),issue('session',['login_state_changed'])];await render();
 expect(host.querySelectorAll('.sync-issue')).toHaveLength(1);
 expect(host.textContent).toContain('先处理配置，再处理登录状态');
 expect(host.textContent).not.toContain('保留本机登录状态');
});
it('treats waiting as automatic progress and blocks unlink confirmation from an outdated revision',async()=>{
 cloud.status={...cloud.status,conflicts:[],waiting:[issue('session',['close_browser'])]};await render();
 expect(host.textContent).toContain('无需选择保留哪一份');expect(host.querySelector('.sync-issue')).toBeNull();
 await click('停止同步');expect(host.querySelector('.sync-confirm')?.textContent).toContain('本机副本和云端数据都会保留');
 await click('取消');expect(cloud.act).not.toHaveBeenCalledWith(expect.objectContaining({type:'detach'}));
 cloud.status={...cloud.status,conflicts:[issue()]};await render();await click('使用云端');
 cloud.status={...cloud.status,conflicts:[{...issue(),remoteRevision:3}]};await render();
 expect(host.querySelector('.sync-confirm')).toBeNull();expect(cloud.act).not.toHaveBeenCalledWith(expect.objectContaining({type:'resolve'}));
});
it('keeps failed resolutions visible instead of reporting success',async()=>{
 await render();await click('使用云端');vi.mocked(cloud.act).mockResolvedValueOnce(null);
 await click('确认使用云端');expect(host.querySelector('.sync-confirm')).not.toBeNull();
 expect(host.textContent).not.toContain('已处理。');
 cloud={...cloud,error:'云端版本再次变化，请重新同步后处理冲突'};await render();
 expect(host.querySelector('.sync-error')?.textContent).toContain('云端版本再次变化');
});
it('does not offer a login-state overwrite when the cloud object is missing',async()=>{
 cloud.status.conflicts=[issue('session',['cloud_object_missing'])];await render();
 expect(host.textContent).toContain('云端记录缺失');expect(host.textContent).not.toContain('保留本机登录状态');
});
it('preserves the saved session preference when editing profile selection',async()=>{
 cloud.status={...cloud.status,sessionSync:false,conflicts:[]};await render();await click('选择同步副本');
 expect(document.activeElement).toBe(host.querySelector('.sync-selection'));
 const session=host.querySelector<HTMLInputElement>('[aria-label="同步网站登录状态"]');expect(session?.checked).toBe(false);
 await click('保存并同步');expect(cloud.act).toHaveBeenCalledWith(expect.objectContaining({type:'setup',sessions:false,localIds:['one']}));
});
it('does not re-import an unchecked bound profile through the cloud selection',async()=>{
 cloud.act=vi.fn(async action=>action.type==='preview'?{localProfiles:[{id:'one',name:'My browser',browser:'chrome',eligible:true,selected:true}],profiles:[{id:'remote',name:'My browser',browser:'chrome',deleted:false,revision:2}]} as never:{} as never);
 cloud.status={...cloud.status,pullCloud:true,conflicts:[]};await render();await click('选择同步副本');
 const check=host.querySelector<HTMLInputElement>('.sync-profile-option input')!;
 await act(async()=>check.click());await click('保存并同步');
 expect(cloud.act).toHaveBeenCalledWith({type:'setup',mode:'merge',localIds:[],cloudIds:[],sessions:true});
});
it('refreshes pulled profiles before allowing the selection editor to save',async()=>{
 cloud.status={...cloud.status,conflicts:[]};await render();
 cloud.act=vi.fn(async action=>action.type==='preview'?{localProfiles:[{id:'one',name:'My browser',browser:'chrome',eligible:true,selected:true},{id:'new',name:'New laptop profile',browser:'edge',eligible:true,selected:true}],profiles:[]} as never:{} as never);
 cloud.status={...cloud.status,bindings:[...cloud.status.bindings!,{localId:'new',cloudId:'new-remote',sessionPending:false,configPending:false}]};await render();
 expect(host.textContent).toContain('New laptop profile');await click('选择同步副本');await click('保存并同步');
 expect(cloud.act).toHaveBeenCalledWith(expect.objectContaining({type:'setup',localIds:['one','new']}));
});
it('offers lease takeover only behind an explanation and inline confirmation',async()=>{
 cloud.status={...cloud.status,conflicts:[],waiting:[issue('session',['lease_held'])]};await render();
 await click('接管此副本的登录数据');expect(host.querySelector('.sync-confirm')?.textContent).toContain('另一台设备的后续更改可能产生冲突');
 expect(cloud.act).not.toHaveBeenCalledWith(expect.objectContaining({type:'resolve'}));await click('确认接管此副本的登录数据');
 expect(cloud.act).toHaveBeenCalledWith({type:'resolve',id:'one',kind:'session',choice:'takeover'});
});
it('preselects eligible profiles for first setup and includes sign-in data',async()=>{
 cloud.status={...cloud.status,setupComplete:false,bindings:[],conflicts:[]};await render();
 expect(host.querySelector<HTMLInputElement>('.sync-profile-option input')?.checked).toBe(true);
 expect(host.querySelector<HTMLInputElement>('[aria-label="同步网站登录状态"]')?.checked).toBe(true);
 await click('保存并同步');expect(cloud.act).toHaveBeenCalledWith(expect.objectContaining({type:'setup',localIds:['one'],sessions:true}));
});
it('translates status, consequences and confirmation buttons in English',async()=>{
 setLanguage('en');await render();await click('Use cloud');
 expect(host.querySelector('.sync-confirm')?.textContent).toContain('overwrite unsynced configuration changes');
 expect(host.querySelector('.sync-confirm')?.textContent).toContain('Confirm cloud configuration');
 expect(host.textContent).not.toMatch(/[\u4e00-\u9fff]/);
});
