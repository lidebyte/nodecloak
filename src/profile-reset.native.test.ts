import {beforeEach,expect,it,vi} from 'vitest';
import {invoke} from '@tauri-apps/api/core';
import {resetProfile} from './browser-profiles';
vi.mock('./bridge',()=>({native:true}));
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn().mockResolvedValue(undefined)}));
beforeEach(()=>vi.clearAllMocks());
it('preserves proxy settings in the native reset command by default',async()=>{
 await resetProfile('selected-profile');expect(invoke).toHaveBeenCalledExactlyOnceWith('reset_profile',{id:'selected-profile',resetProxy:false});
});
it('only requests a native proxy reset when explicitly enabled',async()=>{
 await resetProfile('selected-profile',true);expect(invoke).toHaveBeenCalledExactlyOnceWith('reset_profile',{id:'selected-profile',resetProxy:true});
});
