import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Bot,Globe2,ShoppingBag} from 'lucide-react';
import {native} from './bridge';
import {catalogIconURL} from './browser-site-catalog';
const files=import.meta.glob('../server/internal/api/adminassets/icons/*.svg',{eager:true,query:'?url',import:'default'}) as Record<string,string>;
const assets=Object.fromEntries(Object.entries(files).map(([path,value])=>[path.split('/').pop()!.replace('.svg',''),value]));
const images=new Map<string,Promise<string>>();
export default function BrowserSiteIcon({icon}:{icon:string}){
  const [image,setImage]=useState<string|null>(null);
  const [failed,setFailed]=useState(false);
  useEffect(()=>{
    let current=true;setImage(null);setFailed(false);
    if(assets[icon]||!catalogIconURL(icon))return;
    if(!native){setImage(catalogIconURL(icon));return;}
    let request=images.get(icon);
    if(!request){request=invoke<string>('get_browser_site_icon',{icon});images.set(icon,request);}
    void request.then(url=>{if(current)setImage(url);}).catch(()=>{images.delete(icon);if(current)setFailed(true);});
    return()=>{current=false;};
  },[icon]);
  if(icon==='ai')return <Bot size={17} aria-hidden/>;
  if(icon==='shopping')return <ShoppingBag size={17} aria-hidden/>;
  if(icon==='globe'||failed||(!assets[icon]&&!image))return <Globe2 size={17} aria-hidden/>;
  return <img className="browser-site-brand-icon" src={assets[icon]??image??undefined} alt="" aria-hidden onError={()=>setFailed(true)}/>;
}
