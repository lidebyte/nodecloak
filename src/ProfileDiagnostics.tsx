import {useEffect,useRef,useState} from 'react';
import {Copy,FileText,LoaderCircle,RefreshCw,X} from 'lucide-react';
import {readProfileDiagnostics,type ProfileDiagnostics as Report} from './profile-diagnostics';
import {getDateLocale,t} from './i18n';

export default function ProfileDiagnostics({profile,onClose}:{profile:{id:string;name:string};onClose:()=>void}){
 const dialog=useRef<HTMLDialogElement>(null),sequence=useRef(0);
 const [report,setReport]=useState<Report|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState(''),[copied,setCopied]=useState(false);
 async function refresh(){const request=++sequence.current;setLoading(true);setError('');setCopied(false);try{const next=await readProfileDiagnostics(profile.id);if(request===sequence.current)setReport(next);}catch(e){if(request===sequence.current)setError(String(e instanceof Error?e.message:e));}finally{if(request===sequence.current)setLoading(false);}}
 useEffect(()=>{dialog.current?.showModal();void refresh();return()=>{sequence.current++;};},[profile.id]);
 async function copy(){if(!report)return;try{await navigator.clipboard.writeText(JSON.stringify({...report,name:profile.name},null,2));setCopied(true);}catch{setError(t('复制失败，请检查剪贴板权限'));}}
 const time=(value:string)=>{const d=new Date(value);return Number.isNaN(d.valueOf())?value:d.toLocaleString(getDateLocale());};
 return <dialog ref={dialog} className="app-dialog profile-diagnostics-dialog" aria-labelledby="profile-diagnostics-title" onCancel={e=>{e.preventDefault();onClose();}}>
  <button className="modal-close" aria-label={t('关闭启动日志')} onClick={onClose}><X size={19}/></button>
  <span className="eyebrow">DIAGNOSTICS</span><h2 id="profile-diagnostics-title"><FileText size={22}/>{t('启动日志')}</h2><p className="modal-subtitle">{profile.name}</p>
  <p className="profile-field-hint">{t('显示最近 200 条诊断，包含启动、接口连接与恢复记录。')}</p>
  {report?.logPath&&<p className="profile-log-path">{report.logPath}</p>}
  {error&&<p role="alert" className="profile-error">{t(error)}</p>}
  <div className="profile-log-entries" aria-busy={loading} role="log" aria-label={t('副本诊断记录')}>
   {loading&&!report?<p role="status"><LoaderCircle size={17} className="spin"/>{t('正在读取日志…')}</p>:!report?.entries.length?<p>{t('暂无启动日志')}</p>:[...report.entries].reverse().map((entry,index)=><article key={`${entry.time}-${index}`}><header><time>{time(entry.time)}</time><code>{entry.event}</code></header><p>{t(entry.detail)}</p></article>)}
  </div>
  <div className="modal-actions"><button className="button secondary" disabled={loading} onClick={()=>void refresh()}><RefreshCw size={15} className={loading?'spin':''}/>{t('刷新日志')}</button><button className="button secondary" disabled={!report?.entries.length} onClick={()=>void copy()}><Copy size={15}/>{t(copied?'已复制':'复制日志')}</button><button className="button primary" onClick={onClose}>{t('关闭')}</button></div>
 </dialog>;
}
