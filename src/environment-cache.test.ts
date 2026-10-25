// Cached observations remain scoped, survive reopening and reject malformed storage.
import {describe,it,expect} from 'vitest';
import {createEnvironmentCache,type EnvironmentScope} from './environment-cache';
import type {Scan} from './domain';
const computer:EnvironmentScope={browser:'chrome',profileId:null,native:true};
const scan:Scan={platform:'Windows',browser:'chrome',browserAvailable:true,profilePath:'local',checkedAt:'2026-10-16T12:00:00Z',checks:[{id:'timezone',status:'healthy',value:'UTC',detail:'Saved observation',fixable:false}],ip:null,location:null,latency:null,cliInstalled:false};
const storage=()=>{const values=new Map<string,string>();return {getItem:(key:string)=>values.get(key)??null,setItem:(key:string,value:string)=>{values.set(key,value);}};};
describe('last environment observations',()=>{
 it('forgets a reset profile in memory and storage without clearing other environments',()=>{
  const disk=storage(),cache=createEnvironmentCache(disk),profile={...computer,profileId:'reset-profile'},other={...computer,profileId:'other-profile'};
  cache.saveScan(profile,scan);cache.saveScan(other,scan);cache.saveScan(computer,scan);cache.saveHistory(profile,[]);
  expect('forget' in cache).toBe(true);
  cache.forget(profile);
  expect(cache.readScan(profile)).toBeNull();expect(cache.readHistory(profile)).toBeNull();expect(createEnvironmentCache(disk).readScan(profile)).toBeNull();
  expect(cache.readScan(other)).toEqual(scan);expect(cache.readScan(computer)).toEqual(scan);
 });
 it('restores the last completed scan after recreating the cache',()=>{
  const disk=storage();createEnvironmentCache(disk).saveScan(computer,scan);
  expect(createEnvironmentCache(disk).readScan(computer)).toEqual(scan);
 });
 it('keeps computer, profile, browser and demo observations separate',()=>{
  const cache=createEnvironmentCache(storage());cache.saveScan(computer,scan);
  for(const scope of [{...computer,profileId:'profile-a'},{...computer,native:false}])expect(cache.readScan(scope)).toBeNull();
  cache.saveScan({...computer,profileId:'profile-a'},scan);expect(cache.readScan({...computer,profileId:'profile-b'})).toBeNull();
  expect(cache.readScan({...computer,profileId:'profile-a',browser:'firefox'})).toBeNull();
  expect(cache.readScan({...computer,browser:'firefox'})).toEqual(scan);
 });
 it('falls back to in-memory results if persistent storage is unavailable',()=>{
  const cache=createEnvironmentCache({getItem:()=>{throw Error('blocked');},setItem:()=>{throw Error('full');}});
  cache.saveScan(computer,scan);expect(cache.readScan(computer)).toEqual(scan);
 });
 it('ignores malformed or incompatible saved observations',()=>{
  for(const value of ['invalid',JSON.stringify({version:1,entries:{}}),JSON.stringify({...scan,checks:[{id:'other'}]})]){
   const cache=createEnvironmentCache({getItem:()=>value,setItem:()=>{}});expect(cache.readScan(computer)).toBeNull();
  }
 });
});
