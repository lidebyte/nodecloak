import {useEffect,useRef,useState} from 'react';
import {Check,X} from 'lucide-react';
import {t} from './i18n';
import {validateAvatarFile} from './cloud';
import {cropRect} from './avatarCrop';
export function AccountAvatar({src}:{src?:string;name?:string}){
 return src?<img className="account-avatar" src={src} alt=""/>:<span className="account-avatar account-logo" aria-hidden="true"><img className="mark-light" src="/brand/symbol-positive.svg" alt=""/><img className="mark-dark" src="/brand/symbol-primary.svg" alt=""/></span>;
}
export function AvatarEditor({file,onSave,onClose,onError}:{file:File;onSave:(png:string)=>void;onClose:()=>void;onError:(error:string)=>void}){
 const dialog=useRef<HTMLDialogElement>(null),canvas=useRef<HTMLCanvasElement>(null),drag=useRef<{x:number;y:number;offsetX:number;offsetY:number}|null>(null);
 const [image,setImage]=useState<ImageBitmap|null>(null),[zoom,setZoom]=useState(1),[offset,setOffset]=useState({x:0,y:0});
 useEffect(()=>{dialog.current?.showModal();let disposed=false,bitmap:ImageBitmap|undefined;
  const error=validateAvatarFile(file);
  if(error){onError(error);onClose();return;}
  void createImageBitmap(file).then(value=>{bitmap=value;if(disposed){value.close();return;}if(value.width*value.height>40_000_000){value.close();onError('头像分辨率过大');onClose();return;}setImage(value);}).catch(()=>{if(!disposed){onError('无法处理头像');onClose();}});
  return()=>{disposed=true;bitmap?.close();};
 },[file]);
 useEffect(()=>{if(!image)return;const context=canvas.current?.getContext('2d');if(!context)return;const crop=cropRect(image.width,image.height,zoom,offset.x,offset.y);context.clearRect(0,0,256,256);context.drawImage(image,crop.x,crop.y,crop.size,crop.size,0,0,256,256);},[image,zoom,offset]);
 function save(){if(image&&canvas.current)onSave(canvas.current.toDataURL('image/png'));}
 return <dialog ref={dialog} className="app-dialog avatar-crop-dialog" aria-labelledby="avatar-crop-title" onCancel={event=>{event.preventDefault();onClose();}}>
  <button className="modal-close icon-button" aria-label={t('关闭面板')} onClick={onClose}><X size={18}/></button><h2 id="avatar-crop-title">{t('调整头像')}</h2>
  <canvas ref={canvas} width={256} height={256} className="avatar-crop-preview" aria-label={t('拖动图片调整位置')} onPointerDown={event=>{event.currentTarget.setPointerCapture(event.pointerId);drag.current={x:event.clientX,y:event.clientY,offsetX:offset.x,offsetY:offset.y};}} onPointerMove={event=>{if(!drag.current||!image)return;const size=Math.min(image.width,image.height)/zoom,scale=256/size;setOffset({x:Math.max(-1,Math.min(1,drag.current.offsetX-(event.clientX-drag.current.x)*2/(Math.max(1,image.width-size)*scale))),y:Math.max(-1,Math.min(1,drag.current.offsetY-(event.clientY-drag.current.y)*2/(Math.max(1,image.height-size)*scale)))});}} onPointerUp={()=>drag.current=null} onPointerCancel={()=>drag.current=null}/>
  <label className="avatar-range"><span>{t('缩放')}</span><input type="range" min={1} max={4} step={0.01} value={zoom} onChange={event=>setZoom(Number(event.target.value))}/></label>
  <label className="avatar-range"><span>{t('水平位置')}</span><input type="range" min={-1} max={1} step={0.01} value={offset.x} onChange={event=>setOffset({...offset,x:Number(event.target.value)})}/></label>
  <label className="avatar-range"><span>{t('垂直位置')}</span><input type="range" min={-1} max={1} step={0.01} value={offset.y} onChange={event=>setOffset({...offset,y:Number(event.target.value)})}/></label>
  <div className="account-actions"><button className="button secondary" onClick={onClose}>{t('取消')}</button><button className="button primary" disabled={!image} onClick={save}><Check size={15}/>{t('使用此头像')}</button></div>
 </dialog>;
}
