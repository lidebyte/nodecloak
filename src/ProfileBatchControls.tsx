import {Play,Square,Trash2,RotateCcw,X} from 'lucide-react';
import {t} from './i18n';
import type {ProfileBatchAction} from './profile-batch';
export default function ProfileBatchControls({count,trash,disabled,progress,onAction,onClear}:{count:number;trash:boolean;disabled:boolean;progress:string;onAction:(action:ProfileBatchAction)=>void;onClear:()=>void}){
 if(!count&&!progress)return null;
 return <div className="profile-batch-bar" role="region" aria-label={t('批量管理')}><span role="status">{progress||`${t('已选择')} ${count} ${t('个副本')}`}</span><div>
 {trash?<><button className="button secondary compact" disabled={disabled} onClick={()=>onAction('restore')}><RotateCcw size={14}/>{t('批量恢复')}</button><button className="button secondary compact danger" disabled={disabled} onClick={()=>onAction('purge')}><Trash2 size={14}/>{t('批量彻底删除')}</button></>:<><button className="button secondary compact" disabled={disabled} onClick={()=>onAction('start')}><Play size={14}/>{t('批量启动')}</button><button className="button secondary compact" disabled={disabled} onClick={()=>onAction('close')}><Square size={14}/>{t('批量关闭')}</button><button className="button secondary compact danger" disabled={disabled} onClick={()=>onAction('delete')}><Trash2 size={14}/>{t('批量删除')}</button></>}
 <button className="icon-button" aria-label={t('取消选择')} disabled={disabled} onClick={onClear}><X size={15}/></button></div></div>;
}
