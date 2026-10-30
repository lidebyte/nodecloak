import {useEffect,useRef,useState} from 'react';
import {Check,CheckCheck,ChevronDown,CircleAlert,Clock3,Cloud,CloudDownload,CloudUpload,KeyRound,LoaderCircle,RefreshCw,ShieldCheck} from 'lucide-react';
import type {Controller} from './AccountWorkspace';
import type {CloudAction,CloudConflict,CloudPreview} from './cloud';
import {getDateLocale,t} from './i18n';
import BrowserIcon from './BrowserIcon';
import type {BrowserId} from './domain';
import Toggle from './Toggle';
import './sync.css';

type Choice='local'|'cloud'|'both'|'takeover';
type Confirmation={id:string;stamp:string;title:string;description:string;action:CloudAction};
const conflictStamp=(c:CloudConflict)=>JSON.stringify([c.localId,c.cloudId,c.kind,c.remoteRevision,c.fields]);
const fieldNames:Record<string,string>={'/name':'副本名称','/notes':'备注','/tags':'标签','/proxy':'代理配置','/preferences/language':'浏览器语言',deletion_vs_edit:'删除与修改冲突',first_association:'首次关联冲突',local_config_missing:'本机配置记录缺失',cloud_object_missing:'云端记录缺失',config_changed:'副本配置已变化',login_state_changed:'两台设备的登录状态均有变化',lease_held:'另一台设备正在使用此副本'};
const waitingText:Record<string,string>={close_browser:'关闭此副本后即可同步登录状态。',concurrent_write:'其他设备正在保存，将自动合并后重试。',config_changed:'配置更新后将继续同步。',lease_held:'另一台设备正在使用，结束后即可同步。',lease_or_revision_changed:'等待其他设备完成同步后重试。',deletion_waiting_device:'等待另一台设备结束使用后完成删除。'};

export function SyncPreferences({cloud}:{cloud:Controller}){
 const s=cloud.status;
 const rows=[['autoSync','自动同步','配置修改后自动同步；登录状态在副本关闭后同步。',CloudUpload],['sessionSync','同步网站登录状态','在设备间同步 Cookie 等登录数据，运行中的副本不会被覆盖。',KeyRound],['pullCloud','自动接收新副本','将其他设备新建的云端副本添加到本机。',CloudDownload]] as const;
 return <div className="sync-preferences">{rows.map(([key,title,description,Icon])=><label key={key}><Icon size={18}/><span><strong>{t(title)}</strong><small>{t(description)}</small></span><Toggle aria-label={t(title)} checked={!!s[key]} disabled={cloud.busy} onChange={e=>void cloud.act({type:'preferences',autoSync:!!s.autoSync,sessionSync:!!s.sessionSync,pullCloud:!!s.pullCloud,[key]:e.target.checked})}/></label>)}</div>;
}

export default function SyncWorkspace({cloud,focusProfileId}:{cloud:Controller;focusProfileId?:string}){
 const s=cloud.status;
 const [preview,setPreview]=useState<CloudPreview|null>(null),[previewError,setPreviewError]=useState(false),[loading,setLoading]=useState(false),[previewStamp,setPreviewStamp]=useState('');
 const [editing,setEditing]=useState(false),[local,setLocal]=useState<string[]>([]),[remote,setRemote]=useState<string[]>([]),[sessions,setSessions]=useState(!!s.sessionSync),[receive,setReceive]=useState(s.setupComplete?!!s.pullCloud:true);
 const [confirmation,setConfirmation]=useState<Confirmation|null>(null),[message,setMessage]=useState(''),[operating,setOperating]=useState(false);
 const requested=useRef(false),previewRequest=useRef(false),alive=useRef(true),operation=useRef(false),root=useRef<HTMLDivElement>(null),focused=useRef(''),selection=useRef<HTMLElement>(null),selectionTrigger=useRef<HTMLButtonElement>(null);
 const bindingStamp=JSON.stringify((s.bindings||[]).map(b=>[b.localId,b.cloudId]).sort());
 const busy=cloud.busy||operating;
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
 useEffect(()=>{if(!cloud.busy&&!previewRequest.current&&!previewError&&(!requested.current||previewStamp!==bindingStamp)){requested.current=true;void loadPreview();}},[cloud.busy,bindingStamp,previewStamp,previewError]);
 useEffect(()=>{if(editing&&previewStamp!==bindingStamp){setEditing(false);setMessage('副本列表已变化，请重新选择同步范围。');}},[bindingStamp,previewStamp,editing]);
 useEffect(()=>{if(editing){selection.current?.scrollIntoView?.({block:'start'});selection.current?.focus({preventScroll:true});}},[editing]);
 async function loadPreview(){
  if(previewRequest.current)return null;previewRequest.current=true;setLoading(true);setPreviewError(false);
  try{const result=await cloud.act<CloudPreview>({type:'preview'});if(!alive.current)return null;if(result){setPreview(result);setPreviewStamp(bindingStamp);if(!s.setupComplete){setLocal(result.localProfiles.filter(p=>p.eligible).map(p=>p.id));setRemote(result.profiles.filter(p=>!p.deleted&&!s.bindings?.some(b=>b.cloudId===p.id)).map(p=>p.id));setSessions(true);}}else setPreviewError(true);return result;}
  finally{previewRequest.current=false;if(alive.current)setLoading(false);}
 }
 const issues=Object.values((s.conflicts||[]).reduce<Record<string,CloudConflict[]>>((groups,c)=>{(groups[c.localId]??=[]).push(c);return groups;},{})).map(group=>({item:group.find(c=>c.kind==='config')||group[0],blockedSession:group.some(c=>c.kind==='config')&&group.some(c=>c.kind==='session')}));
 const stamps=new Set([...(s.conflicts||[]).map(conflictStamp),...(s.waiting||[]).map(conflictStamp),...(s.bindings||[]).map(b=>`detach:${b.localId}:${b.cloudId}`)]);
 const confirmationValid=confirmation&&stamps.has(confirmation.stamp);
 useEffect(()=>{if(confirmation&&!stamps.has(confirmation.stamp))setConfirmation(null);},[confirmation,JSON.stringify([...stamps])]);
 useEffect(()=>{
  if(!focusProfileId){focused.current='';return;}
  const target=Array.from(root.current?.querySelectorAll<HTMLElement>('[data-sync-profile]')||[]).find(node=>node.dataset.syncProfile===focusProfileId);
  if(target&&focused.current!==focusProfileId){target.scrollIntoView?.({block:'center'});target.focus({preventScroll:true});focused.current=focusProfileId;}
 },[focusProfileId,s.conflicts,s.waiting,preview]);
 async function run(action:CloudAction,success:string){
  if(operation.current||cloud.busy)return;operation.current=true;setOperating(true);setMessage('');
  try{const result=await cloud.act(action);if(result===null||!alive.current)return;setConfirmation(null);setMessage(success);if(action.type==='setup'){setEditing(false);}if(['setup','resolve','detach','sync'].includes(action.type))await loadPreview();}
  finally{operation.current=false;if(alive.current)setOperating(false);}
 }
 function confirmIssue(item:CloudConflict,choice:Choice,title:string,description:string){setMessage('');setConfirmation({id:item.localId,stamp:conflictStamp(item),title,description,action:{type:'resolve',id:item.localId,kind:item.kind as 'config'|'session',choice}});}
 function stopMissing(item:CloudConflict){setMessage('');setConfirmation({id:item.localId,stamp:conflictStamp(item),title:'停止同步',description:'仅停止这台设备的同步关联，不删除云端副本或本机浏览器数据。以后可重新选择同步。',action:{type:'detach',id:item.localId,removeLocal:false}});}
 function confirmationFor(id:string){return confirmationValid&&confirmation.id===id?<InlineConfirmation confirmation={confirmation} busy={busy} onCancel={()=>setConfirmation(null)} onConfirm={()=>void run(confirmation.action,confirmation.action.type==='detach'?'已停止同步，本机副本和云端数据均已保留。':'已处理。同步状态已更新。')}/>:null;}
 async function startEditing(){setConfirmation(null);setMessage('');const latest=await loadPreview();if(!latest||!alive.current)return;setLocal(latest.localProfiles.filter(p=>p.eligible&&(!s.setupComplete||p.selected)).map(p=>p.id));setRemote(latest.profiles.filter(p=>!p.deleted&&!s.bindings?.some(b=>b.cloudId===p.id)).map(p=>p.id));setSessions(s.setupComplete?!!s.sessionSync:true);setReceive(s.setupComplete?!!s.pullCloud:true);setEditing(true);}
 function select(values:string[],id:string,checked:boolean){return checked?[...new Set([...values,id])]:values.filter(v=>v!==id);}
 const availableRemote=preview?.profiles.filter(p=>!p.deleted&&!s.bindings?.some(b=>b.cloudId===p.id))||[];
 const needsSetup=!s.setupComplete;
 const waiting=s.waiting||[],bindings=s.bindings||[];
 const pending=new Set([...waiting.map(c=>c.localId),...bindings.filter(b=>b.configPending||b.sessionPending).map(b=>b.localId)]).size;
 const title=needsSetup?'选择要同步的副本':issues.length?'有副本需要你处理':s.lastError?'同步暂未完成':pending?'等待同步':!s.autoSync?'自动同步已暂停':!s.lastSync?'可以开始同步':'同步已完成';
 const description=needsSetup?'选择本机要上传的副本，以及要接收的云端副本。':issues.length?'本机数据已保留。请逐个选择要保留的内容，其他副本可继续同步。':s.lastError?'请查看下方具体原因后重试，本机数据会保留。':pending?s.autoSync?'将自动重试，无需选择保留哪一份。':'点击“立即同步”重试，或开启自动同步。':!s.autoSync?'你仍可点击“立即同步”，手动同步已关联的副本。':'在不同设备上接着使用，登录状态会在副本关闭后同步。';
 return <div className="sync-workspace" ref={root}>
  <section className={`sync-overview ${issues.length||s.lastError?'attention':''}`} aria-label={t('同步状态')}>
   <span className="sync-status-icon">{busy?<LoaderCircle className="spin" size={26}/>:issues.length||s.lastError?<CircleAlert size={26}/>:pending?<Clock3 size={26}/>:<Cloud size={26}/>}</span>
   <div className="sync-overview-copy"><span className="eyebrow">NODECLOAK SYNC</span><h2>{t(title)}</h2><p>{t(description)}</p><small>{s.deviceName||t('当前设备')}{s.lastSync&&<> · {t('上次同步')} {new Date(s.lastSync).toLocaleString(getDateLocale())}</>}</small></div>
   {!needsSetup&&<button className="button primary" disabled={busy} onClick={()=>void run({type:'sync'},'同步检查已完成，请查看下方状态。')}><RefreshCw size={16} className={busy?'spin':''}/>{t('立即同步')}</button>}
  </section>
  {message&&<div className="sync-feedback" role="status"><Check size={17}/>{t(message)}</div>}
  {(cloud.error||s.lastError)&&<div className="sync-error" role="alert"><CircleAlert size={18}/><div><strong>{t(cloud.error?'操作未完成':'上次同步未完成')}</strong><p>{t(cloud.error||s.lastError)}</p></div></div>}
  {!needsSetup&&<>
   {!!issues.length&&<section className="sync-section" aria-label={t('需要你处理')}><SectionHeading title="需要你处理" count={issues.length} description="只处理下面这些副本，无需重新设置全部同步。"/>
    {issues.map(({item,blockedSession})=>{
     const fields=item.fields,localMissing=fields.includes('local_config_missing'),missing=localMissing||fields.includes('cloud_object_missing'),config=item.kind==='config',deletion=fields.includes('deletion_vs_edit');
     const cloudProfile=preview?.profiles.find(p=>p.id===item.cloudId),previewCurrent=!!preview&&previewStamp===bindingStamp;
     const canRecover=localMissing&&config&&previewCurrent&&cloudProfile&&!cloudProfile.deleted&&cloudProfile.revision===item.remoteRevision&&!item.localId.startsWith('legacy-');
     const name=localMissing?(cloudProfile?.name||(item.name==='已删除副本'?t('待恢复副本'):item.name)):item.name;
     const blocked=missing||item.kind==='running'||fields.includes('config_changed')||fields.includes('deletion_waiting_device');
     return <article className="sync-issue" data-sync-profile={item.localId} tabIndex={-1} key={item.localId}>
      <div className="sync-item-heading"><div><h3>{name}</h3><p>{t(localMissing?'本机缺少这份副本的配置，需要恢复或停止同步':config?'两台设备对配置有不同修改':item.kind==='running'?'副本正在使用中':'登录状态需要确认')}</p></div><span className="sync-tag warning">{t(config?'配置问题':'登录状态')}</span></div>
      <p className="sync-issue-detail">{fields.map(f=>t(fieldNames[f]||(f.startsWith('/')?'副本设置有不同修改':'此副本需要重新检查同步状态'))).join(' · ')}</p>
      {blockedSession&&<p className="sync-order"><ShieldCheck size={15}/>{t('先处理配置，再处理登录状态')}</p>}
      {localMissing?<>
       <p className="sync-explanation">{t('同步关联仍在，但当前数据目录中没有这份副本的配置记录。这不代表你删除了副本，也不会向云端发送删除。')}</p>
       <div className="sync-recovery-actions">
        {canRecover?<button className="button primary compact" disabled={busy||loading} onClick={()=>confirmIssue(item,'cloud','从云端恢复配置','将云端配置恢复到这台设备，不会清空本机已有的浏览器数据。登录状态将在后续同步时单独核对；若存在不同修改，会保留并提示选择。')}><CloudDownload size={15}/>{t('从云端恢复配置')}</button>:<><p className="cloud-hint">{t(previewCurrent&&cloudProfile?.deleted?'云端副本也已删除，暂时无法恢复配置。可以停止此关联，或从本机配置备份恢复。':'请重新检查同步状态，确认云端仍有可恢复的配置。')}</p><button className="button secondary compact" disabled={busy||loading} onClick={()=>void run({type:'sync'},'同步检查已完成，请查看下方状态。')}>{t('重新检查')}</button></>}
        <button className="button secondary compact" data-action="detach" disabled={busy} onClick={()=>stopMissing(item)}>{t('停止同步')}</button>
       </div>
       <details className="sync-recovery-details"><summary>{t('副本标识')}</summary><p>{item.localId}</p><p>{t('云端标识')} · {item.cloudId}</p></details>
      </>:blocked?<p className="sync-explanation">{t(fields.includes('cloud_object_missing')?'云端记录缺失，请先重新同步配置；本机登录数据已保留。':fields.includes('config_changed')?'请先重新同步此副本的配置。':fields.includes('deletion_waiting_device')?'等待另一台设备释放登录数据后再同步。':'请关闭此副本后重新同步。')}</p>:
       fields.includes('lease_held')?<div className="sync-choice"><button className="button secondary compact" disabled={busy} onClick={()=>confirmIssue(item,'takeover','接管此副本的登录数据','另一台设备的后续更改可能产生冲突。请先确认那台设备已结束使用。')}>{t('接管此副本的登录数据')}</button></div>:
       <><p className="sync-explanation">{t(deletion?'一台设备删除了此副本，另一台设备仍有修改。选择已删除的一方会同时删除另一方；本机删除后可在“最近删除”恢复。':config?'选择一份配置继续使用，或将本机版本另存为一个副本。':'两台设备的登录数据都有变化。请选择要继续使用的登录状态，或保留一个本机副本。')}</p>
        <div className="sync-choices">
         <div><button className="button secondary compact" disabled={busy} onClick={()=>confirmIssue(item,'local',config?'保留本地':'保留本机登录状态',deletion?'将采用本机的配置或删除状态，并覆盖云端配置。若本机已删除，云端也会删除。':config?'保留这台设备的配置，并覆盖云端配置。':'用这台设备的登录数据覆盖云端登录状态。其他设备下次同步时会采用这份数据。')}>{t(config?'保留本地':'保留本机登录状态')}</button><small>{t(config?'用这台设备的配置更新云端':'用本机登录数据更新云端')}</small></div>
         <div><button className="button secondary compact" disabled={busy} onClick={()=>confirmIssue(item,'cloud',config?'使用云端':'使用云端登录状态',deletion?'将采用云端的配置或删除状态。若云端已删除，本机副本也会移到“最近删除”。':config?'使用云端配置，覆盖这台设备上尚未同步的配置修改。':'用云端登录数据替换本机登录状态，当前本机的登录变化将被覆盖。')}>{t(config?'使用云端':'使用云端登录状态')}</button><small>{t(config?'将云端配置应用到这台设备':'将云端登录数据应用到本机')}</small></div>
         <div><button className="button secondary compact" disabled={busy} onClick={()=>confirmIssue(item,'both','保留两份','把本机版本另存为新副本，再将云端版本应用到原副本。若云端已删除，原副本会移到“最近删除”。')}>{t('保留两份')}</button><small>{t('本机另存一份，原副本采用云端')}</small></div>
        </div></>}
      {confirmation?.stamp===conflictStamp(item)&&confirmationFor(item.localId)}
     </article>;
    })}
   </section>}
   {!!waiting.length&&<section className="sync-section" aria-label={t('等待同步')}><SectionHeading title="等待同步" count={new Set(waiting.map(c=>c.localId)).size} description={s.autoSync?'将自动重试，无需选择保留哪一份。':'开启自动同步或点击“立即同步”后重试。'}/><div className="sync-list">{waiting.map(item=><div className="sync-waiting-item" key={`${item.localId}-${item.kind}`}><div className="sync-row" tabIndex={-1} data-sync-profile={item.localId}><Clock3 size={18}/><div><strong>{item.name}</strong><small>{item.fields.map(f=>t(waitingText[f]||'等待下一次同步检查。')).join(' ')}</small></div><span className="sync-tag">{t('等待中')}</span></div>{item.fields.includes('lease_held')&&!issues.some(g=>g.item.localId===item.localId)&&<details className="sync-takeover"><summary>{t('另一台设备已结束使用？')}</summary><p>{t('通常只需等待。若另一台设备已退出但仍无法同步，可以接管登录数据。')}</p><button className="button secondary compact" disabled={busy} onClick={()=>confirmIssue(item,'takeover','接管此副本的登录数据','另一台设备的后续更改可能产生冲突。请先确认那台设备已结束使用。')}>{t('接管此副本的登录数据')}</button></details>}{confirmation?.stamp===conflictStamp(item)&&confirmationFor(item.localId)}</div>)}</div></section>}
   <section className="sync-section"><SectionHeading title="已关联副本" count={bindings.length} description="这些副本已关联云端。停止同步会保留本机副本和云端数据。" action={<button ref={selectionTrigger} className="button secondary compact" disabled={busy||loading||!preview||previewStamp!==bindingStamp} onClick={()=>void startEditing()}>{t('选择同步副本')}</button>}/>
    <div className="sync-list">{bindings.map(b=>{const p=preview?.localProfiles.find(p=>p.id===b.localId),conflict=issues.some(g=>g.item.localId===b.localId),waitingItem=waiting.some(c=>c.localId===b.localId);return <div key={b.localId} className="sync-profile"><div className="sync-row">{p?<BrowserIcon browser={p.browser as BrowserId}/>:<Cloud size={22}/>}<div><strong>{p?.name||preview?.profiles.find(p=>p.id===b.cloudId)?.name||(conflict?t('待恢复副本'):t('正在读取副本名称'))}</strong><small>{t(conflict?'需要处理':waitingItem||b.configPending||b.sessionPending?'等待同步':'已关联云端')}</small></div><button className="button secondary compact" disabled={busy} onClick={()=>{setMessage('');setConfirmation({id:b.localId,stamp:`detach:${b.localId}:${b.cloudId}`,title:'停止同步',description:'本机副本和云端数据都会保留。这台设备不再接收或上传此副本的变化。',action:{type:'detach',id:b.localId,removeLocal:false}});}}>{t('停止同步')}</button></div>{confirmation?.stamp===`detach:${b.localId}:${b.cloudId}`&&confirmationFor(b.localId)}</div>;})}
    {!bindings.length&&<div className="sync-empty"><Cloud size={28}/><strong>{t('还没有关联的副本')}</strong><p>{t('点击“选择同步副本”开始，本机浏览器数据会保留。')}</p></div>}</div>
   </section>
  </>}
  {(needsSetup||editing)&&<section ref={selection} tabIndex={-1} className="sync-selection sync-section"><SectionHeading title={needsSetup?'开始同步':'选择同步副本'} description="勾选要在设备间使用的副本。取消勾选只停止关联，不删除副本。"/>
   {preview&&<>
    <div className="sync-selection-columns"><div><h3><CloudUpload size={17}/>{t('从这台设备上传')}</h3>{preview.localProfiles.length?preview.localProfiles.map(p=><label className="sync-profile-option" key={p.id}><input type="checkbox" disabled={busy||!p.eligible} checked={local.includes(p.id)} onChange={e=>setLocal(select(local,p.id,e.target.checked))}/><span><strong>{p.name}</strong><small>{p.browser}{!p.eligible&&` · ${t('属于其他账号，请先复制')}`}</small></span></label>):<p className="cloud-hint">{t('本机暂无副本')}</p>}</div>
    <div><h3><CloudDownload size={17}/>{t('从云端接收')}</h3><label className="profile-checkbox"><input type="checkbox" checked={receive} disabled={busy} onChange={e=>setReceive(e.target.checked)}/>{t('自动接收新副本')}</label>{receive&&availableRemote.map(p=><label className="sync-profile-option" key={p.id}><input type="checkbox" checked={remote.includes(p.id)} disabled={busy} onChange={e=>setRemote(select(remote,p.id,e.target.checked))}/><span><strong>{p.name}</strong><small>{p.browser}</small></span></label>)}{!availableRemote.length&&<p className="cloud-hint">{t('云端暂无副本')}</p>}</div></div>
    <label className="profile-checkbox"><input aria-label={t('同步网站登录状态')} type="checkbox" checked={sessions} disabled={busy} onChange={e=>setSessions(e.target.checked)}/>{t('同步网站登录状态')}</label><p className="cloud-hint">{t('登录状态在副本关闭后同步；运行中的副本不会被覆盖。')}</p>
    <p className="cloud-hint">{t('保存后将开启自动同步。')}</p><div className="account-actions">{editing&&<button className="button secondary" disabled={busy} onClick={()=>{setEditing(false);selectionTrigger.current?.focus();}}>{t('取消')}</button>}<button className="button primary" disabled={busy||!preview||previewStamp!==bindingStamp} onClick={()=>void run({type:'setup',mode:receive?'merge':'upload',localIds:local,cloudIds:receive?remote:[],sessions},'同步设置已保存，请查看最新同步状态。')}><Check size={16}/>{t('保存并同步')}</button></div>
   </>}
   {needsSetup&&!preview&&loading&&<p className="cloud-hint" role="status">{t('正在读取可同步的副本')}</p>}
   {needsSetup&&<button className="sync-text-button" disabled={busy} onClick={()=>void run({type:'setup',mode:'none',localIds:[],cloudIds:[],sessions:false},'已暂不同步，可随时选择同步副本。')}>{t('暂不同步')}</button>}
  </section>}
  {(loading||previewError)&&<div className="sync-preview-state">{loading?<span><LoaderCircle size={15} className="spin"/>{t('正在读取副本列表')}</span>:<><span>{t('副本列表暂未获取，当前同步状态仍会保留。')}</span><button className="button secondary compact" disabled={busy} onClick={()=>void loadPreview()}>{t('重新获取副本列表')}</button></>}</div>}
  {!needsSetup&&<details className="sync-section sync-options"><summary>{t('同步设置')}<ChevronDown size={16}/></summary><SyncPreferences cloud={cloud}/></details>}
  <p className="sync-footnote"><ShieldCheck size={15}/>{t('同步数据经过加密。不同设备同时修改同一内容时，才需要你选择。')}</p>
 </div>;
}

function SectionHeading({title,description,count,action}:{title:string;description:string;count?:number;action?:React.ReactNode}){return <div className="sync-section-heading"><div><h2>{t(title)}{count!==undefined&&<span>{count}</span>}</h2><p>{t(description)}</p></div>{action}</div>;}
function InlineConfirmation({confirmation,busy,onCancel,onConfirm}:{confirmation:Confirmation;busy:boolean;onCancel:()=>void;onConfirm:()=>void}){
 const ref=useRef<HTMLDivElement>(null);
 useEffect(()=>{ref.current?.focus({preventScroll:true});},[confirmation]);
 return <div className="sync-confirm" role="alert" tabIndex={-1} ref={ref}><strong>{t(confirmation.title)}</strong><p>{t(confirmation.description)}</p><div><button className="button secondary compact" disabled={busy} onClick={onCancel}>{t('取消')}</button><button className="button primary compact" disabled={busy} onClick={onConfirm}>{busy?<LoaderCircle className="spin" size={14}/>:<CheckCheck size={14}/>} {t(`确认${confirmation.title}`)}</button></div></div>;
}
