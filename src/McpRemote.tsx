import {useState} from 'react';
import {ArrowRight,Check,Cloud,Copy,KeyRound,LoaderCircle,Monitor,ShieldCheck} from 'lucide-react';
import {t} from './i18n';
import {mcpRemoteRequest,type McpRemoteRequest,type McpStatus} from './mcp';

export default function McpRemote({status,onPrepare,onRevoke,inspect=mcpRemoteRequest}:{status:McpStatus;onPrepare:(request:McpRemoteRequest)=>void;onRevoke:(id:string)=>void;inspect?:(code:string)=>Promise<McpRemoteRequest>}){
 const [code,setCode]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(''),[copied,setCopied]=useState(false);
 const routes=status.remote?.routes||[];const live=routes.filter(r=>status.grants.some(g=>g.id===r.localGrantId&&!g.revoked&&g.expiresAt>Date.now()/1000));
 const endpoint=live[0]?.endpoint||'https://nodecloak.com';const resource=status.remote?.resource||endpoint.replace(/\/$/,'')+'/mcp';
 const connected=status.enabled&&live.some(r=>status.remote?.activeRoutes?.includes(r.routeId))&&!status.remote?.error&&Date.now()/1000-(status.remote?.lastConnected||0)<20;
 return <div className="mcp-remote">
  <section className="settings-card mcp-remote-intro"><span className="mcp-symbol"><Cloud size={23}/></span><div><span className="eyebrow">REMOTE ACCESS</span><h2>{t('从另一台设备连接。')}</h2><p>{t('AI 连接云端网关，由这台电脑执行获准的浏览器操作。')}</p></div><span className={'mcp-status '+(connected?'completed':'')}>{t(connected?'连接正常':'等待连接')}</span></section>
  <section className="settings-card mcp-pairing"><div className="mcp-section-heading"><KeyRound size={20}/><h3>{t('授权远程客户端')}</h3></div><p>{t('在 AI 客户端添加以下 MCP 地址。打开授权页面后，将页面上的授权码填到这里。')}</p>
   <div className="mcp-endpoint"><code>{resource}</code><button className="button secondary compact" aria-label={t('复制 MCP 地址')} onClick={()=>void navigator.clipboard.writeText(resource).then(()=>setCopied(true)).catch(()=>setError(t('复制失败')))}>{copied?<Check size={15}/>:<Copy size={15}/>} {t(copied?'已复制':'复制')}</button></div>
   <form className="mcp-pairing-form" onSubmit={e=>{e.preventDefault();setBusy(true);setError('');void inspect(code.replace(/[-\s]/g,'').toUpperCase()).then(onPrepare).catch(e=>setError(String(e))).finally(()=>setBusy(false));}}><label className="mcp-field">{t('授权码')}<input aria-label={t('授权码')} autoComplete="off" spellCheck={false} maxLength={16} value={code} onChange={e=>setCode(e.target.value)} placeholder="A1B2 C3D4 E5F6"/></label><button className="button primary compact" disabled={busy||!status.ready||code.replace(/[-\s]/g,'').length!==12}>{busy?<LoaderCircle size={16} className="spin"/>:<ArrowRight size={16}/>} {t('查看授权请求')}</button></form>
   {error&&<p className="mcp-error" role="alert">{t(error)}</p>}<p className="mcp-footnote"><ShieldCheck size={16}/>{t('授权后，网页内容和截图会经过 NodeCloak 网关发送给此 AI 客户端。浏览器与登录数据在设备端使用。')}</p>
  </section>
  <section className="settings-card mcp-remote-routes"><div className="mcp-section-heading"><Monitor size={20}/><h3>{t('这台设备的远程连接')}</h3></div>
   {!live.length?<p>{t('还没有远程连接。每个客户端都需要单独授权。')}</p>:live.map(route=>{const g=status.grants.find(g=>g.id===route.localGrantId)!;return <article className="mcp-route" key={route.routeId}><div><strong>{g.name}</strong><small>{g.profileIds.length} {t('个授权副本')} · {t('有效期至')} {new Date(g.expiresAt*1000).toLocaleDateString()}</small></div><button className="button secondary compact" onClick={()=>onRevoke(g.id)}>{t('撤销授权')}</button></article>})}
   {status.remote?.error&&live.length>0&&<p className="mcp-error" role="status">{t(status.remote.error)}</p>}
  </section>
 </div>;
}
