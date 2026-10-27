import {describe,it,expect} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import type {ComponentProps} from 'react';
import {AccountButton,AccountDialog,CloudSettings} from './AccountWorkspace';
import {SyncPreferences} from './SyncWorkspace';
import {setLanguage} from './i18n';
import type {CloudStatus} from './cloud';

function controller(status:CloudStatus):ComponentProps<typeof CloudSettings>['cloud'] {
 return {status,panel:'sync',busy:false,error:'',googlePending:false,beginGoogle:async()=>null,setError:()=>{},setPanel:()=>{},open:()=>{},act:async()=>null,reload:async()=>{}};
}
const account={id:'test-account',name:'<script>unsafe</script>',email:'test@example.com',avatar:''};
describe('account and sync screens',()=>{
 it('shows the brand avatar and the real Google association below profile and security settings',()=>{
  setLanguage('en');const cloud={...controller({account:{...account,name:'jamesupereal',googleLinked:true,hasPassword:true}}),panel:'account' as const};
  const html=renderToStaticMarkup(<AccountDialog inline cloud={cloud}/>);
  expect(html).toContain('account-avatar account-logo');expect(html).toContain('/brand/symbol-primary.svg');expect(html).toContain('@jamesupereal');
  expect(html).toContain('Unlink');expect(html).not.toContain('Link Google');
  expect(html.indexOf('account-profile-hero')).toBeLessThan(html.indexOf('account-oauth'));
  expect(html).toContain('Change password');expect(html).toContain('Set up 2FA');
  expect(html).not.toContain('<dialog');
 });
 it('disables unlink and 2FA setup until a password exists',()=>{
  setLanguage('en');const cloud={...controller({account:{...account,googleLinked:true,hasPassword:false,recentAuth:false}}),panel:'account' as const};
  const html=renderToStaticMarkup(<AccountDialog inline cloud={cloud}/>);
  expect(html).toContain('Verify with Google again');
  expect(html).toMatch(/<button[^>]*disabled=""[^>]*>[\s\S]*?Unlink/);
  expect(html).toContain('Set a password before unlinking Google');
 });
 it('shows login before sign-in and escapes the user name after sign-in',()=>{
  setLanguage('zh');
  expect(renderToStaticMarkup(<AccountButton cloud={controller({account:null})} onPreferences={()=>{}}/>)).toContain('登录');
  const html=renderToStaticMarkup(<AccountButton cloud={controller({account})} onPreferences={()=>{}}/>);
  expect(html).toContain('&lt;script&gt;unsafe&lt;/script&gt;');
  expect(html).not.toContain('<script>');
 });
 it('keeps the preferences shortcut compact and blocks sync before recovery is saved',()=>{
  const status={account,unlocked:true,setupComplete:true,recoverySaved:false};
  const html=renderToStaticMarkup(<CloudSettings cloud={controller(status)}/>);
  expect((html.match(/<input[^>]*disabled=""/g)||[])).toHaveLength(0);
  expect(html).toMatch(/<button[^>]*disabled=""[^>]*>[\s\S]*?立即同步/);
  const ready=renderToStaticMarkup(<CloudSettings cloud={controller({...status,recoverySaved:true})}/>);
  expect(ready).not.toContain('disabled=""');
  const incomplete=renderToStaticMarkup(<CloudSettings cloud={controller({...status,recoverySaved:true,setupComplete:false})}/>);
  expect((incomplete.match(/<input[^>]*disabled=""/g)||[])).toHaveLength(0);
  expect(incomplete).toContain('请先在管理同步中选择副本');
  expect(incomplete).toMatch(/<button[^>]*disabled=""[^>]*>[\s\S]*?立即同步/);
  const working=renderToStaticMarkup(<CloudSettings cloud={{...controller({...status,recoverySaved:true}),busy:true}}/>);
  expect((working.match(/<input/g)||[])).toHaveLength(0);
  const preferences=renderToStaticMarkup(<SyncPreferences cloud={{...controller({...status,recoverySaved:true}),busy:true}}/>);
  expect((preferences.match(/<input[^>]*disabled=""/g)||[])).toHaveLength(3);
 });
 it('requires vault verification on an approved new device before presenting sync setup',()=>{
  const html=renderToStaticMarkup(<AccountDialog cloud={controller({account,initialized:true,unlocked:false,approvedNeedsVerification:true,fingerprint:'a'.repeat(64)})}/>);
  expect(html).toContain('输入已授权设备上的加密空间校验码');
  expect(html).toContain('恢复密钥');
  expect(html).not.toContain('应用同步设置');
 });
 it('presents recovery acknowledgment before any profile import or export',()=>{
  const html=renderToStaticMarkup(<AccountDialog cloud={controller({account,initialized:true,unlocked:true,recoverySaved:false})}/>);
  expect(html).toContain('保存恢复密钥');
  expect(html).not.toContain('上传本机副本');
  expect(html).not.toContain('应用同步设置');
 });
 it.each([{unlocked:false},{unlocked:true,recoverySaved:false}])('allows opting out before vault or recovery setup: %j',state=>{
  setLanguage('zh');
  const html=renderToStaticMarkup(<AccountDialog cloud={controller({account,setupComplete:false,...state})}/>);
  expect(html).toMatch(/<button[^>]*class="button secondary compact"[^>]*>[\s\S]*?暂不同步<\/button>/);
  expect(html).not.toContain('应用同步设置');
 });
 it('does not offer overwrite actions for a deletion waiting on another device',()=>{
  const html=renderToStaticMarkup(<AccountDialog cloud={controller({account,unlocked:true,recoverySaved:true,setupComplete:true,conflicts:[{localId:'one',cloudId:'two',name:'Deleted profile',kind:'session',fields:['deletion_waiting_device'],remoteRevision:2}]})}/>);
  expect(html).toContain('等待另一台设备释放登录数据');
  expect(html).not.toContain('应用选择');
 });
 it('requires configuration sync before offering a session choice after a fenced write',()=>{
  setLanguage('zh');
  const html=renderToStaticMarkup(<AccountDialog cloud={controller({account,unlocked:true,recoverySaved:true,setupComplete:true,conflicts:[{localId:'one',cloudId:'two',name:'Profile',kind:'session',fields:['config_changed'],remoteRevision:2}]})}/>);
  expect(html).toContain('请先重新同步此副本的配置。');
  expect(html).not.toContain('应用选择');
 });
});
it('shows queued automatic sync separately from conflicts without forcing overwrite choices',()=>{
 setLanguage('zh');
 const status={account,unlocked:true,recoverySaved:true,setupComplete:true,autoSync:true,conflicts:[],waiting:[{localId:'one',cloudId:'two',name:'Waiting profile',kind:'session' as const,fields:['close_browser'],remoteRevision:2}]};
 const html=renderToStaticMarkup(<AccountDialog cloud={controller(status)}/>);
 expect(html).toContain('等待同步');expect(html).toContain('Waiting profile');expect(html).toContain('将自动重试');
 expect(html).not.toContain('待处理冲突');expect(html).not.toContain('应用选择');
 const settings=renderToStaticMarkup(<CloudSettings cloud={controller(status)}/>);expect(settings).toContain('等待自动同步');expect(settings).not.toContain('处理同步冲突');
});
