import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';
import data from './browser-sites.json';

export interface BrowserSite {id:string;name:string;url:string;icon:string;order:number;enabled:boolean}
export interface BrowserSiteGroup {id:string;name:string;nameEn:string;icon:string;order:number;enabled:boolean;sites:BrowserSite[]}
export interface Announcement {id:string;title:string;body:string;revision:number;publishedAt:string}
export interface BrowserSiteCatalog {revision:number;groups:BrowserSiteGroup[];supportURL?:string;announcement?:string;announcements?:Announcement[]}
export const catalogOrigin='https://nodecloak.com';
export const builtinIcons=['ai','shopping','claude','chatgpt','gemini','grok','muse','amazon','ebay','etsy','walmart','shopee','lazada','globe'] as const;
export const bundledCatalog:BrowserSiteCatalog=data;
const idPattern=/^[A-Za-z0-9_-]{1,80}$/;
const uploadPattern=/^\/v1\/catalog\/icons\/[a-f0-9]{64}\.png$/;
export function catalogIconURL(icon:string):string|null {return typeof icon==='string'?(builtinIcons.some(name=>name===icon)?`${catalogOrigin}/v1/catalog/icons/${icon}`:uploadPattern.test(icon)?catalogOrigin+icon:null):null;}
export function safeCatalogURL(value:string):boolean {
  try {
    const url=new URL(value),host=url.hostname.toLowerCase();
    if(url.protocol!=='https:'||url.username||url.password||(url.port&&url.port!=='443'))return false;
    if(!host.includes('.')||host.endsWith('.')||host.includes(':')||/^\d+(?:\.\d+){3}$/.test(host))return false;
    return !['localhost','local','internal','lan','home','test','invalid','onion'].some(suffix=>host===suffix||host.endsWith('.'+suffix));
  } catch {return false;}
}
export function normalizeCatalog(input:unknown):BrowserSiteCatalog|null {
  if(!input||typeof input!=='object')return null;
  const value=input as BrowserSiteCatalog,ids=new Set<string>();
  const text=(v:unknown,max=160)=>typeof v==='string'&&v.trim().length>0&&v.length<=max;
  const entry=(item:BrowserSite|BrowserSiteGroup)=>{
    if(!item||typeof item.id!=='string'||!idPattern.test(item.id)||ids.has(item.id)||!text(item.name)||!catalogIconURL(item.icon)||!Number.isSafeInteger(item.order)||Math.abs(item.order)>100000||typeof item.enabled!=='boolean')return false;
    ids.add(item.id);return true;
  };
  if(!Number.isSafeInteger(value.revision)||value.revision<0||!Array.isArray(value.groups)||value.groups.length>40)return null;
  let count=0;
  for(const group of value.groups){
    if(!entry(group)||!text(group.nameEn)||!Array.isArray(group.sites)||group.sites.length>100)return null;
    for(const site of group.sites)if(!entry(site)||typeof site.url!=='string'||!safeCatalogURL(site.url)||site.url.length>2048)return null;
    count+=group.sites.length;
  }
  if(count>500||(value.supportURL!==undefined&&value.supportURL!==''&&!safeCatalogURL(value.supportURL))||(value.announcement!==undefined&&(typeof value.announcement!=='string'||Array.from(value.announcement).length>2000)))return null;
  if(value.announcements!==undefined){
    if(!Array.isArray(value.announcements)||value.announcements.length>100)return null;
    const announcementIds=new Set<string>();
    for(const a of value.announcements){
      if(!a||typeof a.id!=='string'||!idPattern.test(a.id)||announcementIds.has(a.id)||typeof a.title!=='string'||!a.title.trim()||Array.from(a.title).length>160||typeof a.body!=='string'||!a.body.trim()||Array.from(a.body).length>2000||!Number.isSafeInteger(a.revision)||a.revision<1||typeof a.publishedAt!=='string'||!Number.isFinite(Date.parse(a.publishedAt)))return null;
      announcementIds.add(a.id);
    }
  }
  const order=(a:{order:number},b:{order:number})=>a.order-b.order;
  return {...value,groups:value.groups.filter(g=>g.enabled).map(g=>({...g,sites:g.sites.filter(s=>s.enabled).sort(order)})).sort(order)};
}
let snapshot={catalog:bundledCatalog,ready:false};
const subscribers=new Set<()=>void>();
let request:Promise<void>|null=null;
const storageKey='nodecloak.published-catalog';
async function refreshCatalog(){
  if(request)return request;
  request=(async()=>{
    let next:BrowserSiteCatalog|null=null;
    try {
      if(native)next=normalizeCatalog(await invoke('get_browser_site_catalog'));
      else {
        const response=await fetch(catalogOrigin+'/v1/catalog',{signal:AbortSignal.timeout(8000),credentials:'omit',redirect:'error'});
        if(response.ok&&Number(response.headers.get('content-length')??'0')<=2*1024*1024){const body=await response.text();if(body.length<=2*1024*1024)next=normalizeCatalog(JSON.parse(body));}
      }
    } catch {/* An unavailable publication keeps the last validated catalog. */}
    if(!native){
      if(next){try{localStorage.setItem(storageKey,JSON.stringify(next));}catch{/* Preview storage is optional. */}}
      else try{next=normalizeCatalog(JSON.parse(localStorage.getItem(storageKey)??'null'));}catch{/* Invalid cache falls back to bundled data. */}
    }
    snapshot={catalog:next??snapshot.catalog,ready:true};
    for(const notify of subscribers)notify();
  })().finally(()=>{request=null;});
  return request;
}
export function useBrowserSiteCatalog(){
  const [state,setState]=useState(snapshot);
  useEffect(()=>{
    const update=()=>setState(snapshot);subscribers.add(update);void refreshCatalog();
    const focus=()=>void refreshCatalog();window.addEventListener('focus',focus);
    const timer=window.setInterval(focus,5*60*1000);
    return()=>{subscribers.delete(update);window.removeEventListener('focus',focus);window.clearInterval(timer);};
  },[]);
  return state;
}
export async function openCatalogSupport(){
  if(native)return invoke<void>('open_catalog_support');
  const url=snapshot.catalog.supportURL??bundledCatalog.supportURL;
  if(url&&safeCatalogURL(url))window.open(url,'_blank','noopener,noreferrer');
}
