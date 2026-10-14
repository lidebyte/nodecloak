import {expect,it,vi} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {ProfileAdvanced,ProfilePreview} from './ProfileAdvanced';
import {defaults} from './browser-profiles';
vi.mock('./bridge',()=>({native:false}));
vi.mock('./i18n',()=>({t:(v:unknown)=>v}));
it('offers working shared privacy controls and previews the chosen policies',()=>{
 const draft=defaults();draft.browser='chrome';Object.assign(draft.preferences.advanced,{webRtc:'public',doNotTrack:'off',blockWebgpu:true,blockSpeech:true});
 const html=renderToStaticMarkup(<ProfileAdvanced draft={draft} onChange={()=>{}} match={null} matching={false} onMatch={()=>{}}/>);
 expect(html).toContain('aria-label="WebRTC 策略"');expect(html).toContain('aria-label="Do Not Track"');expect(html).toContain('禁用 WebGPU');
 const preview=renderToStaticMarkup(<ProfilePreview draft={draft}/>);
 expect(preview).toContain('仅公开接口');expect(preview).toContain('Do Not Track');expect(preview).toContain('关闭');expect(preview).toContain('语音列表');
});
it('shows profile fingerprint identity and independent noise controls on all browsers',()=>{
 for(const browser of ['chrome','edge','firefox'] as const){
  const draft=defaults();draft.browser=browser;
  const html=renderToStaticMarkup(<ProfileAdvanced draft={draft} onChange={()=>{}} match={null} matching={false} onMatch={()=>{}}/>);
  expect(html).toContain('重新生成指纹');expect(html).toContain('Canvas 噪音');expect(html).toContain('WebGL 图像噪音');expect(html).toContain('音频噪音');expect(html).toContain('媒体设备标识');
  expect(renderToStaticMarkup(<ProfilePreview draft={draft}/>)).toContain('稳定指纹');
 }
});
