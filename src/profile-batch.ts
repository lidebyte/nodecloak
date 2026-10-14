import {startProfile,closeProfile,deleteProfile,restoreProfile} from './browser-profiles';
export type ProfileBatchAction='start'|'close'|'delete'|'restore'|'purge';
export interface ProfileBatchResult {succeeded:string[];failed:{id:string;error:string}[]}
export async function runProfileBatch(action:ProfileBatchAction,ids:string[],progress?:(done:number,total:number)=>void):Promise<ProfileBatchResult>{
 const unique=[...new Set(ids)];
 if(unique.length>500)throw new Error('一次最多处理 500 个副本');
 const result:ProfileBatchResult={succeeded:[],failed:[]};
 for(const id of unique){
  try{if(action==='start')await startProfile(id);else if(action==='close')await closeProfile(id);else if(action==='restore')await restoreProfile(id);else await deleteProfile(id,action==='purge');result.succeeded.push(id);}
  catch(error){result.failed.push({id,error:error instanceof Error?error.message:String(error)});}
  progress?.(result.succeeded.length+result.failed.length,unique.length);
 }
 return result;
}
