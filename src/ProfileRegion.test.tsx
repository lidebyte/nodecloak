import {describe,expect,it,vi} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import ProfileRegion from './ProfileRegion';
import {ProfilePreview} from './ProfileAdvanced';
import {defaults} from './browser-profiles';

vi.mock('./bridge',()=>({native:false}));
vi.mock('./i18n',()=>({t:(value:unknown)=>value}));

describe('simplified regional settings',()=>{
  it('offers explicit timezone and position-source controls',()=>{
    const draft=defaults();
    draft.preferences.regional.timezoneMode='custom';
    draft.preferences.regional.timezone='America/Los_Angeles';
    draft.preferences.regional.locationMode='custom';
    const html=renderToStaticMarkup(<ProfileRegion draft={draft} onChange={()=>{}} match={null} matching={false} onMatch={()=>{}}/>);
    expect(html).toContain('aria-label="语言"');
    expect(html).toContain('aria-label="地理位置权限"');
    expect(html).toContain('aria-label="时区"');
    expect(html).toContain('地理位置来源');
    expect(html).toContain('自定义时区');
  });
  it('previews the configured timezone and coordinates',()=>{
    const draft=defaults();
    draft.preferences.regional.timezoneMode='custom';
    draft.preferences.regional.timezone='America/Los_Angeles';
    draft.preferences.regional.locationMode='custom';
    const html=renderToStaticMarkup(<ProfilePreview draft={draft}/>);
    expect(html).toContain('America/Los_Angeles');
    expect(html).toContain('定位坐标');
  });
  it('explains the native-language fallback when IP matching enables automation',()=>{
    const draft=defaults();draft.preferences.fingerprint=undefined;draft.preferences.regional.languageMode='ip';
    const firefox=renderToStaticMarkup(<ProfileRegion draft={draft} onChange={()=>{}} match={null} matching={false} onMatch={()=>{}}/>);
    expect(firefox).not.toContain('自动化模式标记');
    draft.browser='chrome';
    const html=renderToStaticMarkup(<ProfileRegion draft={draft} onChange={()=>{}} match={null} matching={false} onMatch={()=>{}}/>);
    expect(html).toContain('若网页验证反复失败，请改为自定义语言后重启副本');
    draft.preferences.regional.languageMode='custom';
    const normal=renderToStaticMarkup(<ProfileRegion draft={draft} onChange={()=>{}} match={null} matching={false} onMatch={()=>{}}/>);
    expect(normal).not.toContain('自动化模式标记');
  });
});
