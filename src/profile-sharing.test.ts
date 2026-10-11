import {describe,expect,it,vi} from 'vitest';
import {effectiveShareStatus,shareDeadline,sharingAction,shareErrorMessage} from './profile-sharing';
import {setLanguage,t} from './i18n';
const api=vi.hoisted(()=>({invoke:vi.fn()}));vi.mock('./bridge',()=>({native:true}));vi.mock('@tauri-apps/api/core',()=>({invoke:api.invoke}));
describe('share deadlines',()=>{
 const now=Date.parse('2026-10-11T14:00:00Z');
 it('starts preset periods from the supplied send time',()=>{expect(shareDeadline('7','',now)).toBe('2026-10-18T14:00:00.000Z');});
 it('rejects past, invalid and over-limit custom deadlines',()=>{expect(()=>shareDeadline('custom','2026-10-10T14:00:00Z',now)).toThrow();expect(()=>shareDeadline('custom','',now)).toThrow();expect(()=>shareDeadline('custom','2028-01-01T00:00:00Z',now)).toThrow('366');});
 it('never reactivates a revoked grant when its deadline is in the future',()=>{expect(effectiveShareStatus({status:'revoked',expiresAt:'2027-01-01T00:00:00Z'},now)).toBe('revoked');expect(effectiveShareStatus({status:'accepted',expiresAt:'2026-10-10T00:00:00Z'},now)).toBe('expired');});
});
it('preserves an accepted local binding from a wrapped native result',async()=>{api.invoke.mockResolvedValue({share:{id:'grant',status:'accepted'},localProfileId:'restricted'});await expect(sharingAction({type:'accept',id:'grant'})).resolves.toEqual({id:'grant',status:'accepted',localProfileId:'restricted'});expect(api.invoke).toHaveBeenCalledWith('sharing_action',{action:{type:'accept',id:'grant'}});});
it('explains how to record a recipient device without sending users to personal cloud sync',()=>{
 const message=shareErrorMessage('云端请求失败 (409): recipient_device_approval_required');
 expect(message).toContain('对方');expect(message).toContain('登录 NodeCloak');expect(message).toContain('接受邀请时');expect(message).not.toContain('管理同步');expect(message).not.toContain('recipient_device');
 setLanguage('en');expect(t(message)).toContain('Your friend');expect(t(message)).toContain('when accepting');expect(t(message)).not.toContain('Manage Sync');setLanguage('zh');
});
it('passes explicit share-only device consent to the native acceptance command',async()=>{api.invoke.mockResolvedValue({id:'grant',status:'accepted'});await sharingAction({type:'accept',id:'grant',authorizeDevice:true});expect(api.invoke).toHaveBeenCalledWith('sharing_action',{action:{type:'accept',id:'grant',authorizeDevice:true}});});
it('gives in-place guidance for authorization and resend guidance for unaddressed devices',()=>{expect(shareErrorMessage('云端请求失败 (403): share_authorization_required')).toContain('确认窗口');expect(shareErrorMessage('云端请求失败 (403): share_device_unavailable')).toContain('重新发送邀请');});
it('keeps unmatched native failures intact instead of misclassifying them as recipient approval',()=>{
 expect(shareErrorMessage(new Error('网络连接不可用'))).toBe('网络连接不可用');
 expect(shareErrorMessage('recipient_device_approval_required is not a native API failure')).toBe('recipient_device_approval_required is not a native API failure');
});
it('guides a recipient with no active device to sign in without requiring personal cloud settings',()=>{
 const message=shareErrorMessage('云端请求失败 (409): recipient_device_unavailable');
 expect(message).toContain('先登录 NodeCloak');expect(message).not.toContain('recipient_device_unavailable');expect(message).not.toContain('云端同步');
});
