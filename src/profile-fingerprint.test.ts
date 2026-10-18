import {expect,it,vi} from 'vitest';
import {defaults,normalizePreferences,requiresBrowserControl} from './browser-profiles';
vi.mock('./bridge',()=>({native:false}));
it('creates independent seeds and does not opt legacy profiles into noise',()=>{
 const a=defaults().preferences as unknown as {fingerprint?:{seed:string;enabled:boolean}};
 const b=defaults().preferences as unknown as {fingerprint?:{seed:string;enabled:boolean}};
 expect(a.fingerprint?.seed).toMatch(/^[0-9a-f]{64}$/);expect(a.fingerprint?.seed).not.toBe(b.fingerprint?.seed);
 expect(a.fingerprint?.enabled).toBe(true);
 const original=defaults().preferences;delete (original as unknown as {fingerprint?:unknown}).fingerprint;
 expect((normalizePreferences(original) as unknown as {fingerprint?:unknown}).fingerprint).toBeUndefined();
});
it('uses native Firefox configuration for manual noise and regional settings',()=>{
 const p=defaults().preferences;
 expect(requiresBrowserControl('firefox',p)).toBe(false);
 expect(requiresBrowserControl('chrome',p)).toBe(true);
 expect(requiresBrowserControl('edge',p)).toBe(true);
 expect(requiresBrowserControl('firefox',{...p,regional:{...p.regional,timezoneMode:'custom',timezone:'Asia/Tokyo'}})).toBe(false);
});
it('does not request browser control for a saved seed with every noise disabled',()=>{
 const p=defaults().preferences;
 p.fingerprint={...p.fingerprint!,canvasNoise:false,fontNoise:false,webglNoise:false,audioNoise:false,mediaNoise:false};
 for(const browser of ['chrome','edge','firefox'] as const){
  expect(requiresBrowserControl(browser,p)).toBe(false);
  for(const flag of ['canvasNoise','fontNoise','webglNoise','audioNoise','mediaNoise'] as const){
   expect(requiresBrowserControl(browser,{...p,fingerprint:{...p.fingerprint!,[flag]:true}})).toBe(browser!=='firefox');
  }
  expect(requiresBrowserControl(browser,{...p,regional:{...p.regional,timezoneMode:'custom',timezone:'Asia/Tokyo'}})).toBe(browser!=='firefox');
 }
});
