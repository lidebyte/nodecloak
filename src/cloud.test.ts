import {describe,it,expect} from 'vitest';
import {syncAvailability,verificationCode,validateAvatarFile} from './cloud';
describe('encrypted cloud UI guards',()=>{
 it('requires login, device authorization and completed setup before sync',()=>{
  expect(syncAvailability({account:null})).toBe('login');
  const account={id:'a',name:'A',email:'a@example.com',avatar:''};
  expect(syncAvailability({account,unlocked:false})).toBe('unlock');
  expect(syncAvailability({account,unlocked:true,setupComplete:false})).toBe('setup');
  expect(syncAvailability({account,unlocked:true,setupComplete:true})).toBe('ready');
 });
 it('does not present an unvalidated device fingerprint as an approval code',()=>{expect(verificationCode('xyz')).toBe('');expect(verificationCode('a'.repeat(52)+'123456789abc')).toBe('1234 5678 9ABC');});
 it('rejects active image formats and oversized avatar inputs before decoding',()=>{expect(validateAvatarFile({type:'image/svg+xml',size:100})).not.toBeNull();expect(validateAvatarFile({type:'image/jpeg',size:9*1024*1024})).not.toBeNull();expect(validateAvatarFile({type:'image/png',size:800})).toBeNull();});
});
