import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';
export interface DiagnosticEntry {time:string;browser:string;event:string;attempt:number;detail:string}
export interface ProfileDiagnostics {profileId:string;name:string;logPath:string;entries:DiagnosticEntry[]}
export async function readProfileDiagnostics(id:string):Promise<ProfileDiagnostics>{
  if(native)return invoke('read_profile_diagnostics',{id});
  return {profileId:id,name:'',logPath:'',entries:[]};
}
