import {expect,it,vi} from 'vitest';
import {defaults,normalizePreferences} from './browser-profiles';
vi.mock('./bridge',()=>({native:false}));
it('creates independent seeds and does not opt legacy profiles into noise',()=>{
 const a=defaults().preferences as unknown as {fingerprint?:{seed:string;enabled:boolean}};
 const b=defaults().preferences as unknown as {fingerprint?:{seed:string;enabled:boolean}};
 expect(a.fingerprint?.seed).toMatch(/^[0-9a-f]{64}$/);expect(a.fingerprint?.seed).not.toBe(b.fingerprint?.seed);
 expect(a.fingerprint?.enabled).toBe(true);
 const original=defaults().preferences;delete (original as unknown as {fingerprint?:unknown}).fingerprint;
 expect((normalizePreferences(original) as unknown as {fingerprint?:unknown}).fingerprint).toBeUndefined();
});
