import {beforeEach,describe,expect,it,vi} from 'vitest';

const mocks=vi.hoisted(()=>({invoke:vi.fn().mockResolvedValue(undefined)}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:mocks.invoke}));
beforeEach(()=>{vi.resetModules();mocks.invoke.mockClear();});

describe('browser website shortcuts',()=>{
  it('opens each website in the selected profile and follows a profile switch',async()=>{
    const {launchBrowser,setActiveProfile}=await import('./bridge');
    setActiveProfile('work-firefox');
    await launchBrowser('firefox','site:chatgpt');
    expect(mocks.invoke).toHaveBeenLastCalledWith('launch_browser',{browser:'firefox',destination:'site:chatgpt',profileId:'work-firefox'});
    setActiveProfile('shopping-chrome');
    await launchBrowser('chrome','site:amazon');
    expect(mocks.invoke).toHaveBeenLastCalledWith('launch_browser',{browser:'chrome',destination:'site:amazon',profileId:'shopping-chrome'});
  });

  it('retains the native browser fallback and local diagnostics destination',async()=>{
    const {launchBrowser,setActiveProfile}=await import('./bridge');
    setActiveProfile(null);
    await launchBrowser('edge','site:gemini');
    expect(mocks.invoke).toHaveBeenLastCalledWith('launch_browser',{browser:'edge',destination:'site:gemini',profileId:null});
    setActiveProfile('work');
    await launchBrowser('edge','diagnostics');
    expect(mocks.invoke).toHaveBeenLastCalledWith('launch_browser',{browser:'edge',destination:'diagnostics',profileId:'work'});
  });
});
