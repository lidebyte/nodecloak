import {beforeEach,expect,it,vi} from 'vitest';
vi.mock('./bridge',()=>({native:false}));
beforeEach(()=>{vi.resetModules();const store=new Map<string,string>();vi.stubGlobal('localStorage',{getItem:(k:string)=>store.get(k)??null,setItem:(k:string,v:string)=>store.set(k,v)});});
it('retains failures while processing other selected profiles and keeps deleted data restorable',async()=>{
 const api=await import('./browser-profiles');const {runProfileBatch}=await import('./profile-batch');
 const [first,second]=await api.listProfiles();await api.startProfile(first.id);
 const result=await runProfileBatch('delete',[first.id,second.id,second.id,'missing']);
 expect(result.succeeded).toEqual([second.id]);expect(result.failed.map(f=>f.id)).toEqual([first.id,'missing']);
 expect((await api.listProfiles()).find(p=>p.id===first.id)?.deletedAt).toBeNull();
 await runProfileBatch('restore',[second.id]);expect((await api.listProfiles()).every(p=>p.deletedAt===null)).toBe(true);
});
