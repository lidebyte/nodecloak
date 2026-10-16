// Persist recent scan observations locally; repair backups remain in their native store.
import {definitions,labels,type BrowserId,type Scan,type RepairRecord} from './domain';
export interface EnvironmentScope {browser:BrowserId;profileId:string|null;native:boolean}
type StorageAccess=Pick<Storage,'getItem'|'setItem'>;
const storageKey='nodecloak.environment-observations.v1';
export const environmentScopeKey=(scope:EnvironmentScope)=>JSON.stringify([scope.native,scope.profileId,scope.profileId===null?null:scope.browser]);
const isScan=(value:unknown):value is Scan=>{
  if(!value||typeof value!=='object')return false;
  const scan=value as Scan;
  return ['chrome','edge','firefox'].includes(scan.browser)&&typeof scan.platform==='string'
    &&typeof scan.browserAvailable==='boolean'&&typeof scan.profilePath==='string'
    &&typeof scan.checkedAt==='string'&&Number.isFinite(Date.parse(scan.checkedAt))
    &&(scan.ip===null||typeof scan.ip==='string')&&(scan.location===null||typeof scan.location==='string')
    &&(scan.latency===null||typeof scan.latency==='number'&&Number.isFinite(scan.latency))&&typeof scan.cliInstalled==='boolean'
    &&Array.isArray(scan.checks)&&scan.checks.length>0&&scan.checks.length<=50
    &&scan.checks.every(check=>check&&Object.hasOwn(definitions,check.id)&&Object.hasOwn(labels,check.status)
      &&typeof check.value==='string'&&typeof check.detail==='string'&&typeof check.fixable==='boolean');
};
export const createEnvironmentCache=(provided?:StorageAccess)=>{
  const memory=new Map<string,Scan>(),histories=new Map<string,RepairRecord[]>();
  const entries=():{key:string;scan:Scan}[]=>{
    try{
      const raw=(provided??localStorage).getItem(storageKey);if(!raw||raw.length>512000)return [];
      const body=JSON.parse(raw);
      if(body.version!==1||!Array.isArray(body.entries)||body.entries.length>20)return [];
      return body.entries.filter((entry:{key?:unknown;scan?:unknown})=>typeof entry?.key==='string'&&entry.key.length<512&&isScan(entry.scan));
    }catch{return [];}
  };
  return {
    readScan:(scope:EnvironmentScope):Scan|null=>{
      const key=environmentScopeKey(scope),scan=memory.get(key)??entries().find(entry=>entry.key===key)?.scan;
      return scan&&(scope.profileId===null||scan.browser===scope.browser)?structuredClone(scan):null;
    },
    saveScan:(scope:EnvironmentScope,scan:Scan)=>{
      if(!isScan(scan)||scan.browser!==scope.browser)return;
      const key=environmentScopeKey(scope);memory.set(key,structuredClone(scan));
      try{(provided??localStorage).setItem(storageKey,JSON.stringify({version:1,entries:[...entries().filter(entry=>entry.key!==key),{key,scan}].slice(-20)}));}catch{/* Keep the successful observation in memory if storage is unavailable. */}
    },
    readHistory:(scope:EnvironmentScope)=>histories.get(environmentScopeKey(scope))??null,
    saveHistory:(scope:EnvironmentScope,records:RepairRecord[])=>{histories.set(environmentScopeKey(scope),records);},
    clearHistories:()=>histories.clear(),
  };
};
