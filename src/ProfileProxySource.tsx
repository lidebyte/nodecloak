import {Link2,PenLine} from 'lucide-react';
import type {ProxyConfig} from './browser-profiles';
import {t} from './i18n';
import SelectMenu from './SelectMenu';
import './proxy-chain.css';
export default function ProfileProxySource({proxy,onChange,disabled=false}:{proxy:ProxyConfig;onChange:(proxy:ProxyConfig)=>void;disabled?:boolean}) {
 const api=proxy.apiUrl!=null;
 return <section aria-label={t('代理来源')}>
  <div className="profile-form proxy-source-form">
   <label>{t('代理来源')}<SelectMenu label={t('代理来源')} disabled={disabled} value={api?'api':'manual'} options={[{value:'manual',label:t('手动填写'),icon:<PenLine size={15}/>},{value:'api',label:t('API 提取'),icon:<Link2 size={15}/>}]} onChange={source=>onChange({...proxy,apiUrl:source==='api'?'':undefined,host:'',port:source==='api'?0:8080})}/></label>
   {api&&<label className="proxy-api-url">{t('代理 API 地址')}<input aria-label={t('代理 API 地址')} type="url" autoComplete="off" spellCheck={false} disabled={disabled} maxLength={4096} placeholder="https://provider.example/extract" value={proxy.apiUrl??''} onChange={e=>onChange({...proxy,apiUrl:e.target.value.trim()})}/></label>}
  </div>
  {api&&<><p className="profile-field-hint">{t('接口应返回纯文本，每行一个 IP:端口；多条时使用第一条，其他内容按异常显示。')}</p><p className="profile-field-hint">{t('每次启动提取一次，运行期间保持同一地址。测试连接会单独提取。')}</p></>}
 </section>;
}
