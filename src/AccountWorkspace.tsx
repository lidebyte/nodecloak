import {useEffect,useId,useRef,useState,type KeyboardEvent} from 'react';
import {listen} from '@tauri-apps/api/event';
import {Check,Cloud,CloudDownload,CloudUpload,KeyRound,LoaderCircle,LogIn,LogOut,Mail,Monitor,RefreshCw,Settings2,ShieldCheck,Trash2,UserRound,X} from 'lucide-react';
import {native} from './bridge';
import {t,getDateLocale} from './i18n';
import {cloudAction,cloudStatus,syncAvailability,verificationCode,type CloudAction,type CloudDevice,type CloudPreview,type CloudStatus} from './cloud';
import SelectMenu from './SelectMenu';
import Toggle from './Toggle';
import {startGooglePolling} from './googlePolling';
import {AccountAvatar as Avatar} from './AvatarEditor';
import {AccountProfile} from './AccountProfile';
import './account.css';

type Panel='account'|'sync'|'devices';
export function useCloudAccount(){
 const [status,setStatus]=useState<CloudStatus>({account:null}),[panel,setPanel]=useState<Panel|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const working=useRef(false),alive=useRef(true);
 const [googlePending,setGooglePending]=useState(false);
 const latest=useRef({act});latest.current={act};
 async function reload(){try{const s=await cloudStatus();if(alive.current)setStatus(s);}catch(e){if(alive.current)setError(String(e));}}
 useEffect(()=>{alive.current=true;void reload();const timer=setInterval(()=>{if(!working.current)void reload();},15000);let stop:(()=>void)|undefined;
  if(native)void listen('cloud-changed',()=>void reload()).then(fn=>{if(!alive.current)fn();else stop=fn;});
  return()=>{alive.current=false;clearInterval(timer);stop?.();};},[]);
 async function act<T=unknown>(action:CloudAction):Promise<T|null>{if(working.current)return null;working.current=true;setBusy(true);setError('');try{const reply=await cloudAction<T>(action);if(alive.current){setStatus(reply.status);if(action.type==='logout'||action.type==='cancelLogin')setGooglePending(false);}return reply.result;}catch(e){if(alive.current){setError(String(e));await reload();}return null;}finally{working.current=false;if(alive.current)setBusy(false);}}
 async function beginGoogle(){const result=await act<{pending:boolean}>({type:'googleStart'});if(alive.current&&result?.pending)setGooglePending(true);return result;}
 // OAuth outlives the profile/device view that started it.
 useEffect(()=>{if(!googlePending)return;return startGooglePolling({
  isBusy:()=>working.current,
  poll:()=>latest.current.act<{pending?:boolean}>({type:'googlePoll'}),
  onFinish:succeeded=>{setGooglePending(false);if(succeeded)setPanel(null);},
 });},[googlePending]);
 function open(p:Panel='account'){setError('');setPanel(p);}
 return {status,panel,busy,error,googlePending,beginGoogle,setError,setPanel,open,act,reload};
}
export type Controller=ReturnType<typeof useCloudAccount>;
export function AccountButton({cloud,onPreferences,onAccountPage,preferencesDisabled=false}:{cloud:Controller;onPreferences:()=>void;onAccountPage?:(page:'account'|'devices')=>void;preferencesDisabled?:boolean}){
 const a=cloud.status.account,id=useId();
 const [expanded,setExpanded]=useState(false);
 const root=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null),menu=useRef<HTMLDivElement>(null);
 useEffect(()=>{setExpanded(false);},[a?.id]);
 useEffect(()=>{
  if(!expanded)return;
  menu.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  function outside(event:PointerEvent){if(event.target instanceof Node&&!root.current?.contains(event.target))setExpanded(false);}
  function escape(event:globalThis.KeyboardEvent){if(event.key==='Escape'){event.preventDefault();setExpanded(false);trigger.current?.focus();}}
  document.addEventListener('pointerdown',outside);document.addEventListener('keydown',escape);
  return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape);};
 },[expanded]);
 function choose(panel:'account'|'devices'){setExpanded(false);if(a&&onAccountPage)onAccountPage(panel);else cloud.open(panel);}
 async function logout(){const result=await cloud.act({type:'logout'});if(result!==null){setExpanded(false);cloud.setPanel(null);}}
 function keyboard(event:KeyboardEvent){
  if(event.key==='Tab'){setExpanded(false);return;}
  if(!['ArrowDown','ArrowUp','Home','End'].includes(event.key))return;
  event.preventDefault();
  const items=Array.from(menu.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')||[]);
  const current=items.findIndex(item=>item===document.activeElement);
  const index=event.key==='Home'?0:event.key==='End'?items.length-1:(current+(event.key==='ArrowDown'?1:-1)+items.length)%items.length;
  items[index]?.focus();
 }
 return <div className="sidebar-account-menu" ref={root}>
  <button ref={trigger} type="button" className="sidebar-account" title={a?.name} disabled={cloud.busy}
   aria-haspopup="menu" aria-expanded={expanded} aria-controls={expanded?id:undefined}
   onClick={()=>setExpanded(value=>!value)}
   onKeyDown={event=>{if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();setExpanded(true);}}}>
   <Avatar src={a?.avatar} name={a?.name}/><span>{a?a.name:t('登录')}</span>
  </button>
  {expanded&&<div ref={menu} id={id} className="profile-menu sidebar-account-options" role="menu" aria-label={t('账号')} onKeyDown={keyboard}>
   {a?<><div className="account-menu-heading"><Avatar src={a.avatar} name={a.name}/><div><strong>{a.name}</strong><small>{a.email}</small></div></div>
   <button type="button" role="menuitem" disabled={cloud.busy} onClick={()=>choose('account')}><UserRound size={16}/>{t('个人资料')}</button>
   <button type="button" role="menuitem" disabled={cloud.busy} onClick={()=>choose('devices')}><Monitor size={16}/>{t('设备')}</button></>:
   <button type="button" role="menuitem" disabled={cloud.busy} onClick={()=>choose('account')}><LogIn size={16}/>{t('登录')}</button>}
   <div className="account-menu-separator" role="separator"/>
   <button type="button" role="menuitem" disabled={cloud.busy||preferencesDisabled} onClick={()=>{setExpanded(false);onPreferences();}}><Settings2 size={16}/>{t('偏好设置')}</button>
   {a&&<><div className="account-menu-separator" role="separator"/><button type="button" role="menuitem" disabled={cloud.busy} onClick={()=>void logout()}><LogOut size={16}/>{t('退出登录')}</button></>}
   {cloud.error&&<p className="profile-error" role="alert">{t(cloud.error)}</p>}
  </div>}
 </div>;
}
export function CloudSettings({cloud}:{cloud:Controller}){
 const s=cloud.status,ready=syncAvailability(s)==='ready';
 function toggle(key:'autoSync'|'sessionSync'|'pullCloud',value:boolean){void cloud.act({type:'preferences',autoSync:!!s.autoSync,sessionSync:!!s.sessionSync,pullCloud:!!s.pullCloud,[key]:value});}
 return <section className="settings-card cloud-settings"><div className="cloud-section-heading"><h2><Cloud size={18}/>{t('云端同步')}</h2><button className="button secondary compact" onClick={()=>cloud.open(s.account?'sync':'account')}>{s.account?<Settings2 size={14}/>:<LogIn size={14}/>} {t(s.account?'管理同步':'登录')}</button></div>
  {s.account?<><div className="cloud-preferences"><label className="cloud-preference-row"><CloudUpload size={16}/><span>{t('自动同步')}</span><Toggle aria-label={t('自动同步')} checked={!!s.autoSync} disabled={cloud.busy} onChange={e=>toggle('autoSync',e.target.checked)}/></label>
   <label className="cloud-preference-row"><KeyRound size={16}/><span>{t('同步网站登录状态')}</span><Toggle aria-label={t('同步网站登录状态')} checked={!!s.sessionSync} disabled={cloud.busy} onChange={e=>toggle('sessionSync',e.target.checked)}/></label>
   <label className="cloud-preference-row"><CloudDownload size={16}/><span>{t('拉取云端新增副本')}</span><Toggle aria-label={t('拉取云端新增副本')} checked={!!s.pullCloud} disabled={cloud.busy} onChange={e=>toggle('pullCloud',e.target.checked)}/></label></div>
   <div className="cloud-settings-actions"><span role="status">{t(!s.unlocked?'请先在管理同步中授权设备':s.recoverySaved===false?'请先在管理同步中保存恢复密钥':!s.setupComplete?'请先在管理同步中选择副本':s.conflicts?.length?'存在待处理冲突':s.lastSync?'上次同步':'尚未同步')}{s.lastSync&&ready&&<> · {new Date(s.lastSync).toLocaleString(getDateLocale())}</>}</span><button className="button secondary compact" disabled={!ready||cloud.busy} onClick={()=>void cloud.act({type:'sync'})}>{cloud.busy?<LoaderCircle size={14} className="spin"/>:<RefreshCw size={14}/>} {t('立即同步')}</button></div>
   {s.lastError&&<p className="profile-error" role="alert">{t(s.lastError)}</p>}
  </>:<p className="cloud-hint">{t('登录后同步浏览器副本和网站登录状态。')}</p>}
  {cloud.error&&<p className="profile-error" role="alert">{t(cloud.error)}</p>}
 </section>;
}

export function AccountDialog({cloud,inline=false}:{cloud:Controller;inline?:boolean}){
 const dialog=useRef<HTMLDialogElement>(null);const s=cloud.status;
 const googlePending=cloud.googlePending;
 const [email,setEmail]=useState(''),[code,setCode]=useState(''),[sent,setSent]=useState(false);
 const [password,setPassword]=useState(''),[passwordMode,setPasswordMode]=useState(false),[twoFactor,setTwoFactor]=useState(false);
 const [recovery,setRecovery]=useState(''),[saved,setSaved]=useState(false),[restoreCode,setRestoreCode]=useState(''),[vaultCode,setVaultCode]=useState('');
 const [exportMessage,setExportMessage]=useState('');
 const [preview,setPreview]=useState<CloudPreview|null>(null),[mode,setMode]=useState<'merge'|'upload'|'none'>('merge'),[local,setLocal]=useState<string[]>([]),[remote,setRemote]=useState<string[]>([]),[sessions,setSessions]=useState(true);
 const [devices,setDevices]=useState<CloudDevice[]>([]),[approvalCode,setApprovalCode]=useState<Record<string,string>>({}),[choices,setChoices]=useState<Record<string,'local'|'cloud'|'both'|'skip'|'takeover'>>({});
 useEffect(()=>{setExportMessage('');if(inline)return;if(cloud.panel){dialog.current?.showModal();}else{dialog.current?.close();setRecovery('');setRestoreCode('');setVaultCode('');setCode('');setPassword('');setTwoFactor(false);setSaved(false);}},[cloud.panel,inline]);
 useEffect(()=>{setPreview(null);setDevices([]);},[s.account?.id]);
 useEffect(()=>{if(cloud.panel==='sync'&&s.unlocked)void loadPreview();if(cloud.panel==='devices'&&s.account)void loadDevices();},[cloud.panel,s.account?.id,s.unlocked]);
 async function loadPreview(){const result=await cloud.act<CloudPreview>({type:'preview'});if(result){setPreview(result);setLocal(result.localProfiles.filter(p=>p.eligible&&(p.selected||!s.setupComplete)).map(p=>p.id));setRemote(result.profiles.filter(p=>!p.deleted).map(p=>p.id));}}
 async function loadDevices(){const result=await cloud.act<{devices:CloudDevice[]}>({type:'devices'});if(result)setDevices(result.devices);}
 async function requestCode(){const result=await cloud.act({type:'requestEmail',email:email.trim()});if(result!==null)setSent(true);}
 async function login(){const result=await cloud.act<{twoFactorRequired?:boolean}>(twoFactor?{type:'verifyTwoFactor',code:code.trim()}:passwordMode?{type:'passwordLogin',email:email.trim(),password}:{type:'verifyEmail',email:email.trim(),code:code.trim()});setCode('');setPassword('');if(result?.twoFactorRequired){setTwoFactor(true);}else if(result!==null){setSent(false);setTwoFactor(false);cloud.setPanel(null);}}
 function beginGoogle(){void cloud.beginGoogle().then(result=>{if(result?.pending){setTwoFactor(false);setCode('');setPassword('');}});}
 async function initialize(){const result=await cloud.act<{recoveryCode:string}>({type:'initialize'});if(result){setRecovery(result.recoveryCode);setSaved(false);}}
 async function showRecovery(){const result=await cloud.act<{recoveryCode:string}>({type:'pendingRecovery'});if(result)setRecovery(result.recoveryCode);}
 async function downloadRecovery(){setExportMessage('');const result=await cloud.act<{saved:boolean}>({type:'saveRecoveryFile',kind:'vault'});if(result!==null)setExportMessage(result.saved?'文件已保存':'已取消保存');}
 function select(values:string[],id:string,checked:boolean){return checked?[...new Set([...values,id])]:values.filter(x=>x!==id);}
 const gate=syncAvailability(s);
 const content=<div className="account-dialog-content">
  {!s.account?<div className="account-login">
   {!native&&<p className="notice">{t('请在桌面客户端登录')}</p>}
   {!twoFactor&&<><button className="button secondary google-login" disabled={!native||cloud.busy||googlePending} onClick={beginGoogle}><GlobeGoogle/> {t(googlePending?'正在等待 Google 登录':'使用 Google 登录')}</button><div className="account-divider">{t('或使用邮箱')}</div></>}
   <form onSubmit={event=>{event.preventDefault();void(twoFactor||passwordMode||sent?login():requestCode());}}>
    {twoFactor?<><p className="cloud-hint">{t('输入验证器验证码或一个未使用的恢复码。')}</p><label className="account-field"><span>{t('验证器验证码或恢复码')}</span><input autoComplete="one-time-code" value={code} onChange={event=>setCode(event.target.value)} maxLength={64} required disabled={cloud.busy}/></label></>:
    <><label className="account-field"><span>{t('邮箱')}</span><input type="email" autoComplete="email" value={email} onChange={event=>{setEmail(event.target.value);setSent(false);}} required disabled={cloud.busy} maxLength={254}/></label>
     {passwordMode?<label className="account-field"><span>{t('密码')}</span><input type="password" autoComplete="current-password" value={password} onChange={event=>setPassword(event.target.value)} maxLength={128} required disabled={cloud.busy}/></label>:
      sent&&<label className="account-field"><span>{t('验证码')}</span><input inputMode="numeric" autoComplete="one-time-code" value={code} onChange={event=>setCode(event.target.value.replace(/\D/g,''))} maxLength={6} minLength={6} required disabled={cloud.busy}/></label>}
    </>}
    <div className="account-actions">{twoFactor?<button type="button" className="button secondary" disabled={cloud.busy} onClick={()=>void cloud.act({type:'cancelLogin'}).then(result=>{if(result!==null){setTwoFactor(false);setCode('');setSent(false);}})}>{t('返回登录')}</button>:
     <><button type="button" className="button secondary" disabled={cloud.busy||googlePending} onClick={()=>{setPasswordMode(!passwordMode);setPassword('');setCode('');setSent(false);}}>{t(passwordMode?'使用邮箱验证码':'使用密码登录')}</button>{sent&&!passwordMode&&<button type="button" className="button secondary" disabled={cloud.busy} onClick={()=>void requestCode()}><Mail size={14}/>{t('重新发送')}</button>}</>}
     <button className="button primary" type="submit" disabled={!native||cloud.busy||googlePending}>{cloud.busy?<LoaderCircle size={15} className="spin"/>:<LogIn size={15}/>} {t(twoFactor?'验证并登录':passwordMode?'登录':sent?'登录 / 注册':'发送验证码')}</button></div>
   </form>
  </div>:cloud.panel==='account'?<AccountProfile key={s.account.id} cloud={cloud} googlePending={googlePending} onGoogle={beginGoogle}/>
:cloud.panel==='devices'?<div className="account-devices"><div className="cloud-section-heading"><span>{t('本机校验码')} <code>{verificationCode(s.fingerprint||'')}</code>{s.vaultCode&&<small className="vault-code">{t('加密空间校验码')} <code>{s.vaultCode.toUpperCase().match(/.{4}/g)?.join(' ')}</code></small>}</span><button className="button secondary compact" disabled={cloud.busy} onClick={()=>void loadDevices()}><RefreshCw size={14}/>{t('刷新')}</button></div>
   {devices.map(d=><div key={d.id} className="account-device"><Monitor size={18}/><div><strong>{d.name}{d.id===s.deviceId&&<small> · {t('当前设备')}</small>}</strong><small>{t(d.status==='pending'?'等待授权':d.status==='approved'?'已授权':'已撤销')}</small>{d.status==='pending'&&d.id!==s.deviceId&&<label className="account-field"><span>{t('输入新设备显示的校验码')}</span><input value={approvalCode[d.id]||''} onChange={e=>setApprovalCode({...approvalCode,[d.id]:e.target.value})} maxLength={14}/></label>}</div>
    {d.status==='pending'&&d.id!==s.deviceId?<button className="button primary compact" disabled={cloud.busy||!s.unlocked||!(approvalCode[d.id]||'').trim()} onClick={()=>void cloud.act({type:'approveDevice',id:d.id,verificationCode:approvalCode[d.id]||''}).then(r=>{if(r!==null)void loadDevices();})}><ShieldCheck size={14}/>{t('授权')}</button>:d.status!=='revoked'&&d.id!==s.deviceId?<button className="button secondary compact" disabled={cloud.busy||!s.unlocked} onClick={()=>void cloud.act<{recoveryCode:string}>({type:'revokeDevice',id:d.id}).then(r=>{if(r){setRecovery(r.recoveryCode);setSaved(false);cloud.setPanel('sync');}})}>{t('撤销授权')}</button>:null}
   </div>)}<p className="cloud-hint">{t('撤销会阻止后续同步，已经下载到设备的数据无法远程收回。')}</p>
  </div>:<div className="account-sync">
   {gate==='unlock'?<div className="account-vault"><ShieldCheck size={28}/>{!s.initialized?<><p>{t('建立加密空间后，保存恢复密钥。')}</p><button className="button primary" disabled={cloud.busy} onClick={()=>void initialize()}><KeyRound size={15}/>{t('建立加密空间')}</button></>:<><p>{t('请在已授权设备上核对并批准此校验码。')}</p><code className="device-verification-code">{verificationCode(s.fingerprint||'')}</code><button className="button secondary" disabled={cloud.busy} onClick={()=>void cloud.act({type:'refresh'})}><RefreshCw size={14}/>{t('检查授权状态')}</button>{s.approvedNeedsVerification&&<><label className="account-field"><span>{t('输入已授权设备上的加密空间校验码')}</span><input value={vaultCode} onChange={e=>setVaultCode(e.target.value)} maxLength={14} autoComplete="off" disabled={cloud.busy}/></label><button className="button primary" disabled={cloud.busy||!vaultCode.trim()} onClick={()=>void cloud.act({type:'confirmDevice',code:vaultCode}).then(r=>{if(r!==null)setVaultCode('');})}>{t('确认加密空间')}</button></>}<label className="account-field"><span>{t('恢复密钥')}</span><input type="password" autoComplete="off" value={restoreCode} onChange={e=>setRestoreCode(e.target.value)} placeholder="nc1_…" disabled={cloud.busy}/></label><button className="button primary" disabled={cloud.busy||!restoreCode} onClick={()=>void cloud.act({type:'recover',code:restoreCode}).then(result=>{if(result!==null)setRestoreCode('');})}>{t('解锁本设备')}</button></>}</div>:
    !s.recoverySaved?<div className="account-recovery"><h3>{t('保存恢复密钥')}</h3><p className="cloud-hint">{t('丢失所有授权设备和恢复密钥后，云端数据无法解密。请勿分享此密钥。')}</p>{recovery?<><textarea rows={3} readOnly value={recovery} aria-label={t('恢复密钥')} spellCheck={false}/><button className="button secondary compact" disabled={cloud.busy} onClick={()=>void downloadRecovery()}><KeyRound size={15}/>{t('保存到文件')}</button>{exportMessage&&<p className="cloud-hint" role="status">{t(exportMessage)}</p>}<label className="profile-checkbox"><input type="checkbox" checked={saved} disabled={cloud.busy} onChange={e=>setSaved(e.target.checked)}/>{t('我已安全保存恢复密钥')}</label><div className="account-actions"><button className="button primary compact" disabled={!saved||cloud.busy} onClick={()=>void cloud.act({type:'acknowledgeRecovery'}).then(r=>{if(r!==null)setRecovery('');})}>{t('继续')}</button></div></>:<button className="button secondary compact" disabled={cloud.busy} onClick={()=>void showRecovery()}>{t('显示恢复密钥')}</button>}</div>:
    <><div className="cloud-section-heading"><span>{t(s.setupComplete?'同步副本':'首次同步')}</span><button className="button secondary compact" disabled={cloud.busy} onClick={()=>void loadPreview()}><RefreshCw size={14}/>{t('刷新')}</button></div>
     <SelectMenu label={t('同步方式')} value={mode} disabled={cloud.busy} onChange={v=>setMode(v as typeof mode)} options={[{value:'merge',label:t('合并并拉取云端'),icon:<CloudDownload size={16}/>},{value:'upload',label:t('仅上传所选副本'),icon:<CloudUpload size={16}/>},{value:'none',label:t('暂不同步'),icon:<Monitor size={16}/>}]}/>
     {mode!=='none'&&<><h3 className="cloud-list-title">{t('上传本机副本')}</h3><div className="cloud-profile-list">{preview?.localProfiles.map(p=><label className="profile-checkbox" key={p.id}><input type="checkbox" disabled={cloud.busy||!p.eligible} checked={local.includes(p.id)} onChange={e=>setLocal(select(local,p.id,e.target.checked))}/><span>{p.name}<small>{p.browser}{!p.eligible&&` · ${t('属于其他账号，请先复制')}`}</small></span></label>)}</div>
      {mode==='merge'&&<><h3 className="cloud-list-title">{t('拉取云端副本')}</h3><div className="cloud-profile-list">{preview?.profiles.filter(p=>!p.deleted).map(p=><label className="profile-checkbox" key={p.id}><input type="checkbox" checked={remote.includes(p.id)} disabled={cloud.busy} onChange={e=>setRemote(select(remote,p.id,e.target.checked))}/><span>{p.name}<small>{p.browser}</small></span></label>)}{preview&&!preview.profiles.some(p=>!p.deleted)&&<p className="cloud-hint">{t('云端暂无副本')}</p>}</div></>}
      <label className="profile-checkbox"><input type="checkbox" checked={sessions} disabled={cloud.busy} onChange={e=>setSessions(e.target.checked)}/>{t('同步网站登录状态')}</label><p className="cloud-hint">{t('登录状态在副本关闭后同步；运行中的副本不会被覆盖。')}</p>
     </>}
     <div className="account-actions"><button className="button primary" disabled={cloud.busy||mode!=='none'&&!preview} onClick={()=>void cloud.act({type:'setup',mode,localIds:mode==='none'?[]:local,cloudIds:mode==='merge'?remote:[],sessions})}><Check size={15}/>{t('应用同步设置')}</button>{s.setupComplete&&<button className="button secondary" disabled={cloud.busy} onClick={()=>void cloud.act({type:'sync'})}><RefreshCw size={15}/>{t('立即同步')}</button>}</div>
     {!!s.conflicts?.length&&<div className="cloud-conflicts"><h3>{t('待处理冲突')}</h3>{s.conflicts.map(conflict=><div className="cloud-conflict" key={`${conflict.localId}-${conflict.kind}`}><strong>{conflict.name}</strong><small>{conflict.fields.map(x=>t(conflictField(x))).join(' · ')}</small>{conflict.fields.includes('config_changed')?<p className="cloud-hint">{t('请先重新同步此副本的配置。')}</p>:conflict.kind==='running'||conflict.fields.includes('deletion_waiting_device')?<p className="cloud-hint">{t('请关闭此副本后重新同步。')}</p>:<div className="cloud-conflict-actions"><SelectMenu label={t('处理方式')} value={choices[`${conflict.localId}-${conflict.kind}`]||'skip'} disabled={cloud.busy} onChange={v=>setChoices({...choices,[`${conflict.localId}-${conflict.kind}`]:v as 'local'|'cloud'|'both'|'skip'|'takeover'})} options={conflict.fields.includes('lease_held')?[{value:'skip',label:t('暂不处理')},{value:'takeover',label:t('接管此副本的登录数据')}]:[{value:'skip',label:t('暂不处理')},{value:'local',label:t('保留本地')},{value:'cloud',label:t('使用云端')},{value:'both',label:t('保留两份')}]}/><button className="button secondary compact" disabled={cloud.busy||!choices[`${conflict.localId}-${conflict.kind}`]||choices[`${conflict.localId}-${conflict.kind}`]==='skip'} onClick={()=>void cloud.act({type:'resolve',id:conflict.localId,kind:conflict.kind as 'config'|'session',choice:choices[`${conflict.localId}-${conflict.kind}`]})}>{t('应用选择')}</button></div>}</div>)}</div>}
     {!!s.bindings?.length&&<div className="cloud-subscriptions"><h3>{t('本机订阅')}</h3>{s.bindings.map(b=><div className="cloud-subscription" key={b.localId}><span>{preview?.localProfiles.find(p=>p.id===b.localId)?.name||b.localId}{(b.sessionPending||b.configPending)&&<small>{t('等待副本关闭')}</small>}</span><button className="button secondary compact" disabled={cloud.busy} onClick={()=>void cloud.act({type:'detach',id:b.localId,removeLocal:false})}>{t('取消关联')}</button><button className="button secondary compact" disabled={cloud.busy} onClick={()=>void cloud.act({type:'detach',id:b.localId,removeLocal:true})}><Trash2 size={14}/>{t('仅移除本机')}</button></div>)}</div>}
    </>}
   {!s.setupComplete&&(gate==='unlock'||!s.recoverySaved)&&<div className="account-actions"><button type="button" className="button secondary compact" disabled={cloud.busy} onClick={()=>void cloud.act({type:'setup',mode:'none',localIds:[],cloudIds:[],sessions:false}).then(result=>{if(result!==null)cloud.setPanel(null);})}><Monitor size={14}/>{t('暂不同步')}</button></div>}
  </div>}
  {cloud.error&&<p className="profile-error" role="alert">{t(cloud.error)}</p>}
  {cloud.busy&&<div className="account-progress" role="status"><LoaderCircle size={14} className="spin"/>{t('正在处理')}</div>}
  </div>;
 if(inline)return <section className="settings-card account-page" aria-label={t(cloud.panel==='devices'?'设备':'个人资料')}>{content}</section>;
 return <dialog ref={dialog} className={`app-dialog account-dialog${s.account?' account-dialog-signed-in':''}`} aria-labelledby="account-title" onCancel={e=>{e.preventDefault();if(!cloud.busy)cloud.setPanel(null);}}>
  <button className="modal-close icon-button" aria-label={t('关闭面板')} disabled={cloud.busy} onClick={()=>cloud.setPanel(null)}><X size={18}/></button>
  <h2 id="account-title">{t(s.account?cloud.panel==='sync'?'云端同步':cloud.panel==='devices'?'设备':'个人资料':'登录 NodeCloak')}</h2>
  {content}
 </dialog>;
}
function GlobeGoogle(){return <span className="google-letter" aria-hidden="true">G</span>;}
function conflictField(path:string){return ({'/name':'副本名称','/notes':'备注','/tags':'标签','/proxy':'代理配置','/preferences/language':'浏览器语言','config_changed':'副本配置已变化','deletion_vs_edit':'删除与修改冲突','first_association':'首次关联冲突','cloud_object_missing':'云端记录缺失','concurrent_write':'云端已有新修改','close_browser':'等待副本关闭','login_state_changed':'两台设备的登录状态均有变化','lease_or_revision_changed':'另一台设备正在使用或已有新登录数据','deletion_waiting_device':'等待另一台设备释放登录数据后完成删除','lease_held':'另一台设备正在使用此副本；接管后该设备的后续更改需要处理冲突'} as Record<string,string>)[path]||path;}
